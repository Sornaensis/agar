//! A derived, compact execution graph. The authored graph remains authoritative
//! for mutation, source spans, inspection and checkpoint serialization.
use super::*;

#[derive(Debug, Clone)]
struct ExecutableNode {
    op: Op,
    ty: Type,
}

#[derive(Debug, Clone)]
pub(super) struct ExecutionPlan {
    nodes: Box<[ExecutableNode]>,
    pub(super) output_roots: [Option<usize>; OUTPUT_COUNT],
    pub(super) writes: Box<[WriteRule]>,
    pub(super) extra_roots: Box<[usize]>,
    /// All heap storage owned by the plan, including newly folded strings.
    /// Strings shared with the authored graph are already charged there.
    pub(super) allocated_bytes: usize,
}

impl ExecutionPlan {
    pub(super) fn compile(
        nodes: &[Node],
        output_roots: &[Option<usize>; OUTPUT_COUNT],
        writes: &[WriteRule],
        extra_roots: &[usize],
    ) -> Box<Self> {
        // Nodes are already topological. Only constant-only operations execute
        // here; placeholders for dynamic nodes are never read by constant work.
        let mut constant = Vec::with_capacity(nodes.len());
        let mut values = Vec::with_capacity(nodes.len());
        let inputs = Inputs::default();
        let memory = Registers::default();
        for node in nodes {
            let (children, count) = node.op.children();
            let is_constant = !matches!(
                node.op,
                Op::Sensor(_) | Op::Pool(_) | Op::Built(_) | Op::Register(_) | Op::Populated(_)
            ) && children[..count].iter().all(|&child| constant[child]);
            values.push(if is_constant {
                evaluate_operation(&node.op, node.ty, &inputs, &memory, &values)
            } else {
                Value::Boolean(false)
            });
            constant.push(is_constant);
        }

        // A folded root is a single value, so its old expression subtree no
        // longer needs runtime slots. References simply alias their child slot.
        let mut reached = vec![false; nodes.len()];
        let mut stack: Vec<_> = output_roots.iter().flatten().copied().collect();
        stack.extend_from_slice(extra_roots);
        for rule in writes {
            stack.extend(rule.value);
            stack.extend(rule.guard);
        }
        while let Some(index) = stack.pop() {
            if reached[index] {
                continue;
            }
            reached[index] = true;
            if let Op::Reference(child) = nodes[index].op {
                stack.push(child);
            } else if !constant[index] {
                let (children, count) = nodes[index].op.children();
                stack.extend_from_slice(&children[..count]);
            }
        }

        let mut slots = vec![usize::MAX; nodes.len()];
        let mut executable = Vec::new();
        for (index, node) in nodes.iter().enumerate() {
            if !reached[index] {
                continue;
            }
            if let Op::Reference(child) = node.op {
                slots[index] = slots[child];
                continue;
            }
            let slot = |child: usize| {
                debug_assert_ne!(slots[child], usize::MAX);
                slots[child]
            };
            let op = if constant[index] {
                Op::Constant(values[index].clone())
            } else {
                match &node.op {
                    Op::Sensor(sensor) => Op::Sensor(*sensor),
                    Op::Pool(index) => Op::Pool(*index),
                    Op::Built(index) => Op::Built(*index),
                    Op::Register(key) => Op::Register(*key),
                    Op::Populated(key) => Op::Populated(*key),
                    Op::Negate(child) => Op::Negate(slot(*child)),
                    Op::Not(child) => Op::Not(slot(*child)),
                    Op::Binary(op, a, b) => Op::Binary(*op, slot(*a), slot(*b)),
                    Op::Choose(c, a, b) => Op::Choose(slot(*c), slot(*a), slot(*b)),
                    Op::Call(function, args) => {
                        let mut mapped = [0; 3];
                        for i in 0..function.arity() {
                            mapped[i] = slot(args[i]);
                        }
                        Op::Call(*function, mapped)
                    }
                    Op::Constant(_) | Op::Reference(_) => unreachable!(),
                }
            };
            slots[index] = executable.len();
            executable.push(ExecutableNode { op, ty: node.ty });
        }
        let output_roots = output_roots.map(|root| root.map(|index| slots[index]));
        let extra_roots: Box<[_]> = extra_roots.iter().map(|&index| slots[index]).collect();
        let writes: Box<[_]> = writes
            .iter()
            .map(|rule| WriteRule {
                register: rule.register,
                value: rule.value.map(|index| slots[index]),
                guard: rule.guard.map(|index| slots[index]),
            })
            .collect();
        let mut strings = HashSet::new();
        // The temporary constant results can share allocations with both the
        // authored graph and one another; charge each additional allocation once.
        for node in nodes {
            if let Op::Constant(Value::Text(text)) = &node.op {
                strings.insert(Rc::as_ptr(text) as *const u8);
            }
        }
        let nodes = executable.into_boxed_slice();
        let mut extra_string_bytes = 0;
        for node in &nodes {
            if let Op::Constant(Value::Text(text)) = &node.op {
                if strings.insert(Rc::as_ptr(text) as *const u8) {
                    extra_string_bytes += text.len() + 2 * std::mem::size_of::<usize>();
                }
            }
        }
        let allocated_bytes = std::mem::size_of::<Self>()
            + nodes.len() * std::mem::size_of::<ExecutableNode>()
            + writes.len() * std::mem::size_of::<WriteRule>()
            + extra_roots.len() * std::mem::size_of::<usize>()
            + extra_string_bytes;
        Box::new(Self {
            nodes,
            output_roots,
            writes,
            extra_roots,
            allocated_bytes,
        })
    }

    pub(super) fn evaluate(&self, inputs: &Inputs, memory: &Registers, values: &mut Vec<Value>) {
        values.clear();
        values.reserve(self.nodes.len());
        for node in &self.nodes {
            values.push(evaluate_operation(
                &node.op, node.ty, inputs, memory, values,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::{Reader, Writer};

    const STATEFUL: &str = r#"
let unused = noise(time, energy) + length("unread");
let old = ri001;
let saturated = (2147483647 + 1) * 2147483647;
let fixed_vector = normalize(vec(3, 4)) + vec(-0.0, sqrt(-1));
let fixed_text = "雪" + "🌱";
let old_alias = old;
ri001 = if old_alias < 3 then old_alias + 1 else -2;
rf001 = if rb001 then float(saturated) else energy / 0 when light.valid;
rb001 = !rb001;
rs001 = rs001 + fixed_text when !chemical.valid;
rv001 = if chemical.valid then normalize(flow.vector) else fixed_vector;
clear ri001 when old == 2;
clear rf001 when !light.valid;
clear rs001 when length(rs001) > 25;
clear rb999 when !populated(rs001);
thrust = clamp(energy / 100 + float(old) / 50, 0, 1);
turn = if rb001 then x(rv001) else sin(age);
divide = rb001 && populated(ri001);
target.core.count = max(0, old);
target.core.size = max(0.04, abs(rf001));
express.core = if chemical.valid then 1 else 0;
scan.target = if populated(rs001) then rs001 else fixed_text;
scan.width = 2 * 3.141592653589793;
"#;

    fn checkpoint_memory(memory: &Registers) -> Vec<u8> {
        let mut writer = Writer::new();
        memory.write_checkpoint(&mut writer);
        writer.0
    }

    fn output_float_bits(output: &Outputs) -> Vec<u64> {
        let mut numbers = vec![
            output.thrust,
            output.turn,
            output.grow,
            output.photosynthesize,
            output.bite,
            output.defend,
            output.repair,
            output.absorb,
            output.feed,
            output.scan_bearing,
            output.scan_width,
        ];
        numbers.extend(output.digest);
        numbers.extend(output.body.target_sizes);
        numbers.extend(output.body.build);
        numbers.extend(output.body.recycle);
        numbers.into_iter().map(f64::to_bits).collect()
    }

    fn compare_with_authored_evaluator(program: &Program) {
        let mut writer = Writer::new();
        program.write_checkpoint(&mut writer);
        let restored = Program::read_checkpoint(&mut Reader::new(&writer.0)).unwrap();
        let mut rewritten = Writer::new();
        restored.write_checkpoint(&mut rewritten);
        assert_eq!(
            rewritten.0, writer.0,
            "derived plans must not alter checkpoints"
        );
        assert_eq!(restored.graph_json(), program.graph_json());

        let mut reference_memory = Registers::default();
        for (name, value) in [
            ("ri001", Value::Integer(i32::MAX)),
            ("rf001", Value::Scalar(-0.0)),
            ("rb001", Value::Boolean(false)),
            ("rb999", Value::Boolean(false)),
            ("rs001", Value::Text(Rc::from(""))),
            ("rv001", Value::Vector([-0.0, 0.0])),
        ] {
            reference_memory
                .entries
                .insert(Register::parse(name).unwrap(), value);
        }
        let mut optimized_memory = reference_memory.clone();
        let mut restored_memory = reference_memory.clone();
        let mut full_values = Vec::new();
        // Reuse one scratch across two programs and independent cells so neither
        // folded results nor compact slots can accidentally carry old cell state.
        let mut scratch = EvalScratch::default();
        for tick in 0..36 {
            let inputs = Inputs {
                energy: [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0, 0.0, 37.0][tick % 6],
                age: tick as f64 * 0.13,
                time: tick as f64 / 30.0,
                light_valid: tick % 3 != 0,
                chemical_valid: tick % 4 == 0,
                flow_vector: if tick % 5 == 0 {
                    [f64::NAN, f64::NEG_INFINITY]
                } else {
                    [tick as f64 * 0.2, -1.7]
                },
                organelle: std::array::from_fn(|i| OrganelleInput {
                    count: (tick + i) as i32 % 4,
                    mass: (tick + i) as f64 * 0.001,
                    capacity: ((tick + i) % 5) as f64,
                    building: if tick % 2 == 0 { 0.0 } else { 0.01 },
                    damaged: 0.0,
                }),
                ..Inputs::default()
            };
            program.evaluate_nodes(&inputs, &reference_memory, &mut full_values);
            let expected = Program::outputs_for(&program.output_roots, &full_values);
            Program::commit_writes(&program.writes, &mut reference_memory, &full_values);
            let actual = program.evaluate_cell(&inputs, &mut optimized_memory, &mut scratch);
            let reloaded = restored.evaluate_cell(&inputs, &mut restored_memory, &mut scratch);
            assert_eq!(actual, expected, "optimized outputs at tick {tick}");
            assert_eq!(reloaded, expected, "restored outputs at tick {tick}");
            assert_eq!(output_float_bits(&actual), output_float_bits(&expected));
            assert_eq!(output_float_bits(&reloaded), output_float_bits(&expected));
            let expected_memory = checkpoint_memory(&reference_memory);
            assert_eq!(
                checkpoint_memory(&optimized_memory),
                expected_memory,
                "memory at tick {tick}"
            );
            assert_eq!(
                checkpoint_memory(&restored_memory),
                expected_memory,
                "restored memory at tick {tick}"
            );
        }
    }

    #[test]
    fn runtime_prunes_dead_nodes_and_folds_constants_without_changing_inspection() {
        let mut source = String::new();
        for index in 0..200 {
            source.push_str(&format!("let dead{index} = sin(time + {index});\n"));
        }
        source.push_str(
            "let fixed = clamp(1e300 * 1e300, -1, 1);\n\
             let alias = fixed;\n\
             rs001 = \"雪\" + \"🌱\"; rv001 = normalize(vec(3, 4));\n\
             thrust = alias; turn = -0.0; divide = false;",
        );
        let program = Program::compile(&source).unwrap();
        let mut scratch = EvalScratch::default();
        let mut memory = Registers::default();
        program.evaluate_cell(&Inputs::default(), &mut memory, &mut scratch);
        assert!(program.nodes.len() > 800);
        assert_eq!(
            scratch.values.len(),
            5,
            "only the five folded roots need slots"
        );
        assert!(program
            .execution
            .nodes
            .iter()
            .all(|node| matches!(node.op, Op::Constant(_))));
        let preview =
            Program::evaluation_json(&source, &Inputs::default(), &Registers::default(), 1, 1, 0)
                .unwrap();
        assert!(preview.contains("\"name\":\"dead199\""));
        compare_with_authored_evaluator(&program);
    }

    #[test]
    fn compact_execution_matches_authored_graph_across_types_guards_and_restore() {
        for source in [STATEFUL, DEFAULT_PROGRAM] {
            compare_with_authored_evaluator(&Program::compile(source).unwrap());
        }
    }

    #[test]
    fn mutated_execution_plans_match_authored_graph_over_multiple_ticks() {
        for source in [STATEFUL, DEFAULT_PROGRAM] {
            for kind in [
                MutationKind::Any,
                MutationKind::Constant,
                MutationKind::Expression,
                MutationKind::Reference,
            ] {
                for seed in 0..24 {
                    if let Some(mutation) = Program::mutate_source(source, seed, kind) {
                        compare_with_authored_evaluator(&mutation.program);
                    }
                }
            }
        }
    }
}
