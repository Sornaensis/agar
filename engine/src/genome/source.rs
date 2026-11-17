//! Transient source maps for single-site mutation and read-only evaluation.
//! The executable graph/checkpoint layout stays independent of editor metadata.
use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct NodeLocation {
    pub from: usize,
    pub to: usize,
    pub head: Location,
}

struct ParsedSource {
    program: Program,
    nodes: Vec<NodeLocation>,
    bindings: Vec<Location>,
    writes: Vec<Location>,
}

impl ParsedSource {
    fn parse(source: &str) -> Result<Self, String> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(format!("program exceeds {MAX_SOURCE_BYTES} bytes."));
        }
        let mut parser = Parser::new(lex(source).map_err(|error| error.to_string())?);
        let program = parser.program().map_err(|error| error.to_string())?;
        Ok(Self {
            program,
            nodes: parser.node_locations,
            bindings: parser.binding_locations,
            writes: parser.write_locations,
        })
    }
}

enum ExpressionChange {
    Head(&'static str),
    Wrap(&'static str),
}

fn precedence(operator: Binary) -> u8 {
    match operator {
        Binary::Or => 1,
        Binary::And => 2,
        Binary::Equal | Binary::NotEqual => 3,
        Binary::Less | Binary::LessEqual | Binary::Greater | Binary::GreaterEqual => 4,
        Binary::Add | Binary::Subtract => 5,
        Binary::Multiply | Binary::Divide => 6,
    }
}

fn expression_changes(program: &Program, index: usize) -> Vec<ExpressionChange> {
    let node = &program.nodes[index];
    let mut options = Vec::new();
    match node.op {
        Op::Binary(current, a, b) => {
            // A same-precedence token replacement keeps every operand's parse
            // tree intact without adding parentheses throughout the program.
            for operator in Binary::ALL {
                if operator != current
                    && precedence(operator) == precedence(current)
                    && binary_type(operator, program.nodes[a].ty, program.nodes[b].ty)
                        == Some(node.ty)
                {
                    options.push(ExpressionChange::Head(operator.symbol()));
                }
            }
        }
        Op::Call(current, args) => {
            let types = args[..current.arity()]
                .iter()
                .map(|&i| program.nodes[i].ty)
                .collect::<Vec<_>>();
            for function in Function::ALL {
                if function != current
                    && function.arity() == current.arity()
                    && function.ty(&types) == Some(node.ty)
                {
                    options.push(ExpressionChange::Head(function.name()));
                }
            }
        }
        Op::Negate(_) => options.push(ExpressionChange::Head(if node.ty == Type::Vector {
            ""
        } else {
            "+"
        })),
        Op::Not(_) => options.push(ExpressionChange::Head("")),
        _ => {}
    }
    match node.ty {
        Type::Bool => options.push(ExpressionChange::Wrap("!")),
        Type::Integer | Type::Number | Type::Vector => options.push(ExpressionChange::Wrap("-")),
        Type::String => {} // identity(text) would only add a redundant wrapper.
    }
    options
}

impl Program {
    /// Patch one typed source node in the inherited program. Comments, names,
    /// statement order and every byte outside that node are retained verbatim.
    /// Recompilation validates the patch and supplies fresh graph/node indices.
    pub fn mutate_source(source: &str, seed: u64, requested: MutationKind) -> Option<Mutation> {
        let parsed = ParsedSource::parse(source).ok()?;
        let program = &parsed.program;
        let reachable = program.reachable();
        let mut candidates = Vec::new();
        for (index, node) in program.nodes.iter().enumerate() {
            if !reachable[index] {
                continue;
            }
            if matches!(requested, MutationKind::Any | MutationKind::Constant)
                && matches!(node.op, Op::Constant(_))
            {
                candidates.push((index, MutationKind::Constant));
            }
            if matches!(requested, MutationKind::Any | MutationKind::Expression)
                && !expression_changes(program, index).is_empty()
            {
                candidates.push((index, MutationKind::Expression));
            }
            if matches!(requested, MutationKind::Any | MutationKind::Reference)
                && matches!(
                    node.op,
                    Op::Reference(_) | Op::Sensor(_) | Op::Register(_) | Op::Populated(_)
                )
            {
                candidates.push((index, MutationKind::Reference));
            }
        }
        if candidates.is_empty() {
            return None;
        }
        let mut rng = MutationRng::new(seed);
        let (index, kind) = candidates[rng.index(candidates.len())];
        let node = &program.nodes[index];
        let location = parsed.nodes[index];
        let (from, to, replacement) = match kind {
            MutationKind::Constant => {
                let Op::Constant(value) = &node.op else {
                    unreachable!()
                };
                // The folded minimum integer spans two tokens. Leave a comment
                // between its sign and magnitude untouched rather than erase it.
                if matches!(value, Value::Integer(i32::MIN))
                    && source[location.from..location.to].contains('#')
                {
                    return None;
                }
                // Ordinary negative literals are a unary node plus a positive
                // literal. Preserve that existing sign node: changing zero to
                // a negative spelling here would silently insert another node.
                // i32::MIN is the parser's one folded negative literal; its
                // mutation becomes one nonnegative integer literal as well.
                let changed = match mutate_constant(value, &mut rng) {
                    Value::Integer(number) => Value::Integer(number.saturating_abs()),
                    Value::Scalar(number) => Value::Scalar(number.abs()),
                    value => value,
                };
                (location.from, location.to, changed.source())
            }
            MutationKind::Expression => {
                let changes = expression_changes(program, index);
                match &changes[rng.index(changes.len())] {
                    ExpressionChange::Head(head) => {
                        (location.head.from, location.head.to, (*head).to_string())
                    }
                    ExpressionChange::Wrap(operator) => (
                        location.from,
                        location.to,
                        format!("{operator}({})", &source[location.from..location.to]),
                    ),
                }
            }
            MutationKind::Reference => {
                if let Op::Populated(register) = node.op {
                    // populated() is a boolean leaf in the runtime graph, but
                    // its address is the editable reference. Keep call syntax
                    // and intervening comments/whitespace verbatim.
                    let tokens = lex(&source[location.from..location.to]).ok()?;
                    let token = tokens.iter().find(|token| matches!(&token.kind, Kind::Name(name) if Register::parse(name).is_some()))?;
                    let mut index = rng.index(REGISTER_COUNT - 1) as u16 + 1;
                    if index >= register.index {
                        index += 1;
                    }
                    let replacement = Register {
                        ty: register.ty,
                        index,
                    }
                    .name();
                    (
                        location.from + token.location.from,
                        location.from + token.location.to,
                        replacement,
                    )
                } else {
                    let choices = program.reference_options(index);
                    if choices.is_empty() {
                        return None;
                    }
                    let replacement = match &choices[rng.index(choices.len())] {
                        Op::Sensor(sensor) => sensor.name().to_string(),
                        Op::Register(register) => register.name(),
                        Op::Populated(register) => format!("populated({})", register.name()),
                        Op::Reference(target) => program.bindings[..node.scope]
                            .iter()
                            .find(|binding| binding.node == *target)?
                            .name
                            .clone(),
                        _ => unreachable!(),
                    };
                    (location.from, location.to, replacement)
                }
            }
            MutationKind::Any => unreachable!(),
        };
        if source[from..to] == replacement {
            return None;
        }
        let length = source
            .len()
            .checked_sub(to - from)?
            .checked_add(replacement.len())?;
        if length > MAX_SOURCE_BYTES {
            return None;
        }
        let mut patched = String::with_capacity(length);
        patched.push_str(&source[..from]);
        patched.push_str(&replacement);
        patched.push_str(&source[to..]);
        let compiled = Self::compile(&patched).ok()?;
        if matches!(kind, MutationKind::Constant | MutationKind::Reference) {
            // A one-leaf mutation must leave the entire remaining graph intact,
            // not merely yield some other well-typed parse of the patched text.
            if compiled.nodes.len() != program.nodes.len()
                || compiled.nodes.iter().zip(&program.nodes).enumerate().any(
                    |(other, (new, old))| {
                        new.ty != old.ty
                            || new.scope != old.scope
                            || (other != index && new.op != old.op)
                    },
                )
                || compiled.nodes[index].op == program.nodes[index].op
            {
                return None;
            }
        }
        Some(Mutation {
            program: compiled,
            source: patched,
            kind,
        })
    }

    /// Preview all pure expressions using current signals and memory. The same
    /// node evaluator as a tick runs, but writes/clears are only described.
    pub fn evaluation_json(
        source: &str,
        inputs: &Inputs,
        memory: &Registers,
        cell_id: u32,
        genome_id: u32,
        tick: u64,
    ) -> Result<String, String> {
        let parsed = ParsedSource::parse(source)?;
        let program = &parsed.program;
        let map = SourceMap::new(source);
        let mut values = Vec::new();
        program.evaluate_nodes(inputs, memory, &mut values);
        let range = |from: usize, to: usize| {
            format!("\"from\":{},\"to\":{}", map.offsets[from], map.offsets[to])
        };
        let mut json = format!(
            "{{\"version\":1,\"cellId\":{cell_id},\"genomeId\":{genome_id},\"time\":{},\"tick\":{tick},\"semantics\":\"current-snapshot\",\"source\":{},\"expressions\":[",
            inputs.time, quote(source),
        );
        for (index, node) in program.nodes.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            let location = parsed.nodes[index];
            json.push_str(&format!(
                "{{\"id\":{index},{},\"type\":{},\"value\":{}}}",
                range(location.from, location.to),
                quote(node.ty.label()),
                values[index].json()
            ));
        }
        json.push_str("],\"bindings\":[");
        for (index, binding) in program.bindings.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            let location = parsed.bindings[index];
            json.push_str(&format!(
                "{{\"name\":{},{},\"type\":{},\"value\":{}}}",
                quote(&binding.name),
                range(location.from, location.to),
                quote(program.nodes[binding.node].ty.label()),
                values[binding.node].json()
            ));
        }
        json.push_str("],\"signals\":[");
        for (index, sensor) in Sensor::ALL.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "{{\"name\":{},\"type\":{},\"value\":{}}}",
                quote(sensor.name()),
                quote(sensor.ty().label()),
                sensor.read(inputs).convert(sensor.ty()).json()
            ));
        }
        json.push_str("],\"outputs\":[");
        for (index, root) in program.output_indices().iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            let ty = output_type(index);
            let value = program.output_value(index, &values);
            let offsets = root.map_or_else(
                || "\"from\":null,\"to\":null".to_string(),
                |root| {
                    let location = parsed.nodes[root];
                    range(location.from, location.to)
                },
            );
            json.push_str(&format!(
                "{{\"name\":{},\"type\":{},\"value\":{},{offsets}}}",
                quote(OUTPUT_NAMES[index]),
                quote(ty.label()),
                value.json()
            ));
        }
        json.push_str("],\"writes\":[");
        let clearing: HashSet<_> = program
            .writes
            .iter()
            .filter(|rule| {
                rule.value.is_none() && rule.guard.is_none_or(|guard| values[guard].boolean())
            })
            .map(|rule| rule.register)
            .collect();
        for (index, rule) in program.writes.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            let location = parsed.writes[index];
            let kind = if rule.value.is_some() {
                "write"
            } else {
                "clear"
            };
            let guard = rule.guard.is_none_or(|guard| values[guard].boolean());
            let applies = guard && (rule.value.is_none() || !clearing.contains(&rule.register));
            let value = rule.value.map_or_else(
                || "null".to_string(),
                |value| values[value].clone().convert(rule.register.ty).json(),
            );
            json.push_str(&format!("{{\"name\":{},\"kind\":{},\"type\":{},\"value\":{value},\"guard\":{guard},\"applies\":{applies},{}}}",
                quote(&rule.register.name()), quote(kind), quote(rule.register.ty.label()), range(location.from, location.to)));
        }
        json.push_str("]}");
        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "# café 🌱 — keep this layout\r\nlet reserve = energy * 0.25;  # useful name\r\nlet heading = sin(age + id);\r\nri001 = ri001 + 1 when !rb001;\r\nclear rb001 when age > 12;\r\nthrust = clamp(reserve, 0, 1);\r\nturn = heading;\r\ndivide = energy > 100 && chemical.valid; # end\r\n";

    fn one_changed_location(
        source: &str,
        changed: &str,
        locations: impl Iterator<Item = (usize, usize)>,
    ) -> bool {
        locations.into_iter().any(|(from, to)| {
            changed.len() >= from + source.len() - to
                && changed.starts_with(&source[..from])
                && changed.ends_with(&source[to..])
                && changed != source
        })
    }

    #[test]
    fn preview_reports_integer_counts_string_tokens_and_full_scan_width() {
        let source = "thrust=0;turn=0;divide=false;target.flagellum.count=2;scan.target=object.token;scan.width=20;rs001=contact.token;";
        let inputs = Inputs {
            object_token: Rc::from("track:9"),
            contact_token: Rc::from("track:4"),
            ..Inputs::default()
        };
        let memory = Registers::default();
        let preview = Program::evaluation_json(source, &inputs, &memory, 7, 3, 9).unwrap();
        assert!(preview
            .contains("\"name\":\"target.flagellum.count\",\"type\":\"integer\",\"value\":2"));
        assert!(
            preview.contains("\"name\":\"scan.target\",\"type\":\"string\",\"value\":\"track:9\"")
        );
        assert!(preview.contains(&format!(
            "\"name\":\"scan.width\",\"type\":\"scalar\",\"value\":{}",
            std::f64::consts::TAU
        )));
        assert!(preview.contains(
            "\"name\":\"rs001\",\"kind\":\"write\",\"type\":\"string\",\"value\":\"track:4\""
        ));
        assert_eq!(memory.populated_count(), 0);
    }

    #[test]
    fn constants_and_references_edit_one_original_leaf_and_preserve_layout() {
        let parsed = ParsedSource::parse(SOURCE).unwrap();
        for kind in [MutationKind::Constant, MutationKind::Reference] {
            for seed in 0..300 {
                let mutation = Program::mutate_source(SOURCE, seed, kind).unwrap();
                assert_eq!(mutation.kind, kind);
                assert!(one_changed_location(
                    SOURCE,
                    &mutation.source,
                    parsed
                        .program
                        .nodes
                        .iter()
                        .enumerate()
                        .filter_map(|(index, node)| {
                            let matching = if kind == MutationKind::Constant {
                                matches!(node.op, Op::Constant(_))
                            } else {
                                matches!(
                                    node.op,
                                    Op::Sensor(_)
                                        | Op::Register(_)
                                        | Op::Reference(_)
                                        | Op::Populated(_)
                                )
                            };
                            matching.then_some((parsed.nodes[index].from, parsed.nodes[index].to))
                        })
                ));
                for unchanged in [
                    "# café 🌱 — keep this layout\r\n",
                    "  # useful name\r\n",
                    "# end\r\n",
                    "let reserve =",
                    "let heading =",
                ] {
                    assert!(
                        mutation.source.contains(unchanged),
                        "lost {unchanged:?}: {}",
                        mutation.source
                    );
                }
                assert!(!mutation.source.contains("_g"));
                assert_eq!(
                    mutation.program.bindings.len(),
                    parsed.program.bindings.len()
                );
            }
        }
    }

    #[test]
    fn descendants_keep_their_inherited_source_and_rebuild_scopes() {
        let mut source = SOURCE.to_string();
        let mut applied = 0;
        for seed in 1..=600 {
            let kind = [
                MutationKind::Constant,
                MutationKind::Expression,
                MutationKind::Reference,
            ][seed as usize % 3];
            let Some(mutation) = Program::mutate_source(&source, seed * 97, kind) else {
                continue;
            };
            assert!(mutation
                .source
                .starts_with("# café 🌱 — keep this layout\r\nlet reserve ="));
            assert!(mutation.source.ends_with("; # end\r\n"));
            assert_eq!(
                mutation
                    .program
                    .bindings
                    .iter()
                    .map(|binding| binding.name.as_str())
                    .collect::<Vec<_>>(),
                ["reserve", "heading"]
            );
            assert!(!mutation.source.contains("_g"));
            assert_eq!(
                mutation.program.graph_json(),
                Program::compile(&mutation.source).unwrap().graph_json()
            );
            source = mutation.source;
            applied += 1;
        }
        assert!(applied > 400, "{applied}");
    }

    #[test]
    fn expression_changes_preserve_precedence_operands_and_comments() {
        let source = "let a=energy;let b=age;let c=body.area;\nrf001=a + b * # keep the operand comment\n c;thrust=0;turn=0;divide=false;";
        let mut addition = false;
        let mut multiplication = false;
        for seed in 0..1000 {
            let mutation = Program::mutate_source(source, seed, MutationKind::Expression).unwrap();
            assert!(mutation.source.contains("# keep the operand comment\n"));
            if mutation.source == source.replace("a + b", "a - b") {
                let root = mutation.program.writes[0].value.unwrap();
                let Op::Binary(Binary::Subtract, a, product) = mutation.program.nodes[root].op
                else {
                    panic!("regrouped addition")
                };
                assert!(matches!(mutation.program.nodes[a].op, Op::Reference(_)));
                assert!(matches!(
                    mutation.program.nodes[product].op,
                    Op::Binary(Binary::Multiply, _, _)
                ));
                addition = true;
            }
            if mutation.source == source.replace("b *", "b /") {
                let root = mutation.program.writes[0].value.unwrap();
                let Op::Binary(Binary::Add, _, product) = mutation.program.nodes[root].op else {
                    panic!("regrouped multiplication")
                };
                assert!(matches!(
                    mutation.program.nodes[product].op,
                    Op::Binary(Binary::Divide, _, _)
                ));
                multiplication = true;
            }
        }
        assert!(addition && multiplication);
        let source = "rf001=-(energy + age) * body.area;rb001=!(chemical.valid || heat.valid) && flow.valid;thrust=0;turn=0;divide=false;";
        for seed in 0..200 {
            let mutation = Program::mutate_source(source, seed, MutationKind::Expression).unwrap();
            if mutation.source.contains("rf001=+(energy + age)") {
                let root = mutation.program.writes[0].value.unwrap();
                let Op::Binary(Binary::Multiply, sum, _) = mutation.program.nodes[root].op else {
                    panic!("unary removal regrouped product")
                };
                assert!(matches!(
                    mutation.program.nodes[sum].op,
                    Op::Binary(Binary::Add, _, _)
                ));
            }
            if mutation
                .source
                .contains("rb001=(chemical.valid || heat.valid)")
            {
                let root = mutation.program.writes[1].value.unwrap();
                let Op::Binary(Binary::And, condition, _) = mutation.program.nodes[root].op else {
                    panic!("unary removal regrouped conjunction")
                };
                assert!(matches!(
                    mutation.program.nodes[condition].op,
                    Op::Binary(Binary::Or, _, _)
                ));
            }
        }
    }

    #[test]
    fn missing_outputs_and_dead_bindings_are_not_inserted_or_pruned() {
        let source = "# retain an unused definition\nlet spare=123;thrust=energy;turn=age;divide=chemical.valid;";
        assert!(Program::mutate_source(source, 1, MutationKind::Constant).is_none());
        for seed in 0..50 {
            let mutation = Program::mutate_source(source, seed, MutationKind::Any).unwrap();
            assert!(mutation
                .source
                .starts_with("# retain an unused definition\nlet spare=123;"));
            assert!(!mutation.source.contains("grow"));
            assert!(mutation.program.output_roots[3].is_none());
        }
    }

    #[test]
    fn large_program_can_mutate_without_expanding_to_generated_bindings() {
        let source = format!(
            "thrust={};turn=0;divide=false;",
            vec!["energy"; 3000].join("+")
        );
        assert!(Program::compile(&source).unwrap().to_source().is_none());
        for seed in 0..20 {
            let mutation = Program::mutate_source(&source, seed, MutationKind::Constant).unwrap();
            assert!(mutation.source.len() < source.len() + 32);
            assert_eq!(mutation.source.matches("energy").count(), 3000);
        }
    }

    #[test]
    fn source_spans_use_utf16_and_preview_keeps_current_memory_unchanged() {
        let source = "# café 🌱\r\nlet next = ri001 + 1;\nlet branch = if rb001 then energy * 2 else 7;\nri001=next;clear ri001 when rb001;rs001=\"🌿\";rv001=vec(2,3);\nthrust=branch;turn=-2;divide=false;";
        let mut memory = Registers::default();
        memory
            .entries
            .insert(Register::parse("ri001").unwrap(), Value::Integer(4));
        memory
            .entries
            .insert(Register::parse("rb001").unwrap(), Value::Boolean(true));
        let original = memory.clone();
        let inputs = Inputs {
            energy: 11.0,
            time: 2.0,
            ..Inputs::default()
        };
        let json = Program::evaluation_json(source, &inputs, &memory, 9, 3, 60).unwrap();
        assert_eq!(memory, original);
        assert!(json.contains("\"semantics\":\"current-snapshot\""));
        assert!(json.contains("\"cellId\":9,\"genomeId\":3,\"time\":2,\"tick\":60"));
        assert!(json.contains("\"name\":\"thrust\",\"type\":\"scalar\",\"value\":1"));
        assert!(json.contains("\"name\":\"turn\",\"type\":\"scalar\",\"value\":-1"));
        assert!(json.contains(
            "\"name\":\"grow\",\"type\":\"scalar\",\"value\":0,\"from\":null,\"to\":null"
        ));
        assert!(json.contains("\"name\":\"ri001\",\"kind\":\"write\",\"type\":\"integer\",\"value\":5,\"guard\":true,\"applies\":false"));
        assert!(json.contains("\"name\":\"ri001\",\"kind\":\"clear\",\"type\":\"integer\",\"value\":null,\"guard\":true,\"applies\":true"));
        assert!(json.contains("\"value\":[2,3]"));
        let byte = source.find("ri001 + 1").unwrap();
        let from = source[..byte].encode_utf16().count();
        let to = from + "ri001 + 1".len();
        assert!(json.contains(&format!(
            "\"from\":{from},\"to\":{to},\"type\":\"integer\",\"value\":5"
        )));
        let byte = source.find("energy * 2").unwrap();
        let from = source[..byte].encode_utf16().count();
        let to = from + "energy * 2".len();
        assert!(json.contains(&format!(
            "\"from\":{from},\"to\":{to},\"type\":\"scalar\",\"value\":22"
        )));
        assert_eq!(
            json,
            Program::evaluation_json(source, &inputs, &memory, 9, 3, 60).unwrap()
        );
    }

    #[test]
    fn parser_ranges_cover_functions_unary_min_integer_and_nested_expressions() {
        let source = "let count=-2147483648;let value=sin(energy + age * 2);let test=populated(ri001);thrust=value;turn=- (energy + age);divide=test;";
        let parsed = ParsedSource::parse(source).unwrap();
        for (index, node) in parsed.program.nodes.iter().enumerate() {
            let span = parsed.nodes[index];
            let text = &source[span.from..span.to];
            match node.op {
                Op::Constant(Value::Integer(i32::MIN)) => assert_eq!(text, "-2147483648"),
                Op::Call(Function::Sin, _) => assert_eq!(text, "sin(energy + age * 2)"),
                Op::Populated(_) => assert_eq!(text, "populated(ri001)"),
                Op::Negate(_) => assert_eq!(text, "- (energy + age)"),
                _ => {}
            }
            assert!(span.from < span.to && span.to <= source.len());
        }
        let source = "thrust=(energy)+(age);turn=0;divide=false;";
        let parsed = ParsedSource::parse(source).unwrap();
        let root = parsed.program.output_roots[0].unwrap();
        assert_eq!(
            &source[parsed.nodes[root].from..parsed.nodes[root].to],
            "(energy)+(age)"
        );
    }

    #[test]
    fn numeric_constant_changes_modify_exactly_one_node_even_at_zero_and_signed_limits() {
        let source = "rf001=0.0;ri001=0;ri002=-2147483648;rf002=-1.0;thrust=0;turn=0;divide=false;";
        let parent = Program::compile(source).unwrap();
        for seed in 0..300 {
            let mutation = Program::mutate_source(source, seed, MutationKind::Constant).unwrap();
            assert_eq!(mutation.program.nodes.len(), parent.nodes.len());
            let changed = parent
                .nodes
                .iter()
                .zip(&mutation.program.nodes)
                .filter(|(a, b)| a.op != b.op)
                .count();
            assert_eq!(changed, 1);
            assert!(parent
                .nodes
                .iter()
                .zip(&mutation.program.nodes)
                .all(|(a, b)| a.ty == b.ty && a.scope == b.scope));
        }
    }

    #[test]
    fn populated_reference_mutations_preserve_comments_inside_the_call() {
        let source = "thrust=0;turn=0;divide=populated( # retained\n rb001 );";
        for seed in 0..100 {
            let mutation = Program::mutate_source(source, seed, MutationKind::Reference).unwrap();
            assert!(mutation
                .source
                .starts_with("thrust=0;turn=0;divide=populated( # retained\n rb"));
            assert!(mutation.source.ends_with(" );"));
            assert!(!mutation.source.contains("rb001"));
            assert_eq!(mutation.program.nodes.len(), 3);
        }
        let source = "ri001=- # keep\n2147483648;thrust=energy;turn=age;divide=chemical.valid;";
        assert!(Program::mutate_source(source, 1, MutationKind::Constant).is_none());
    }
}
