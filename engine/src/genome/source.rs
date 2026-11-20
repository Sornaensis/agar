//! Transient source maps for single-site mutation and read-only evaluation.
//! The executable graph/checkpoint layout stays independent of editor metadata.
use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct NodeLocation {
    pub from: usize,
    pub to: usize,
    pub head: Location,
    pub authored: bool,
}

struct ParsedSource {
    program: Program,
    nodes: Vec<NodeLocation>,
    bindings: Vec<Location>,
    writes: Vec<Location>,
    messages: [Option<Location>; 4],
    initializers: [Option<Location>; 4],
    genes: Vec<cellular::GeneLocation>,
    geometry: Vec<cellular::GeometryLocation>,
    ingredients: Vec<Location>,
    targets: Vec<Option<Location>>,
    lifetimes: Vec<Location>,
}

impl ParsedSource {
    fn parse(source: &str) -> Result<Self, String> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(format!("program exceeds {MAX_SOURCE_BYTES} bytes."));
        }
        let mut parser = Parser::new(lex(source).map_err(|error| error.to_string())?);
        let program = parser.program_for(true).map_err(|error| error.to_string())?;
        Ok(Self {
            program,
            nodes: parser.node_locations,
            bindings: parser.binding_locations,
            writes: parser.write_locations,
            messages: parser.message_locations,
            initializers: parser.initializer_locations,
            genes: parser.gene_locations,
            geometry: parser.geometry_locations,
            ingredients: parser.ingredient_locations,
            targets: parser.target_locations,
            lifetimes: parser.lifetime_locations,
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
                if program.cellular.is_none()
                    && matches!(function, Function::Activate | Function::Repress)
                {
                    continue;
                }
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
        Type::Count => {}, // Saturating unsigned negation supplies no useful mutation.
        Type::Integer | Type::Number | Type::Vector => options.push(ExpressionChange::Wrap("-")),
        Type::String => {} // identity(text) would only add a redundant wrapper.
    }
    options
}

// Only swap the substrate of the same operation. Keep installation roles
// identical so a binding-site change cannot add an unrelated organelle.
fn specificity_options(recipe: &AssemblyDef) -> impl Iterator<Item = &'static str> {
    let current = match recipe.kind {
        AssemblyKind::Mechanism(kind) => Some(kind),
        _ => None,
    };
    crate::cellular::MechanismKind::ALL.into_iter().filter_map(move |candidate| {
        let existing = current?;
        (candidate != existing && candidate.constructor() == existing.constructor()
            && candidate.required_roles() == existing.required_roles())
            .then(|| candidate.target()).flatten()
    })
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
            if !reachable[index] || !parsed.nodes[index].authored {
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
                    Op::Reference(_)
                        | Op::Sensor(_)
                        | Op::Pool(_)
                        | Op::Concentration(_)
                        | Op::Built(_)
                        | Op::Peer(_)
                        | Op::Receptor(_, _)
                        | Op::Register(_)
                        | Op::Populated(_)
                )
            {
                candidates.push((index, MutationKind::Reference));
            }
        }
        if matches!(requested, MutationKind::Any | MutationKind::Constant) {
            for (role, location) in parsed.initializers.iter().enumerate() {
                if location.is_some() { candidates.push((program.nodes.len() + role, MutationKind::Constant)); }
            }
        }
        if let Some(cellular) = &program.cellular {
            for index in 0..cellular.genes.len() {
                if matches!(requested, MutationKind::Any | MutationKind::GeneSilence) {
                    candidates.push((index, MutationKind::GeneSilence));
                }
                if cellular.genes.len() < cellular::MAX_GENES
                    && matches!(requested, MutationKind::Any | MutationKind::GeneDuplicate)
                {
                    candidates.push((index, MutationKind::GeneDuplicate));
                }
            }
            if matches!(requested, MutationKind::Any | MutationKind::Turnover) {
                candidates.extend((0..parsed.lifetimes.len()).map(|i| (i, MutationKind::Turnover)));
            }
            if matches!(requested, MutationKind::Any | MutationKind::Ingredient) {
                candidates.extend((0..parsed.ingredients.len()).map(|i| (i, MutationKind::Ingredient)));
            }
            if matches!(requested, MutationKind::Any | MutationKind::Specificity) {
                for (i, target) in parsed.targets.iter().enumerate() {
                    if target.is_some() && specificity_options(&cellular.definition.assemblies[i]).next().is_some() {
                        candidates.push((i, MutationKind::Specificity));
                    }
                }
            }
            if matches!(requested, MutationKind::Any | MutationKind::Geometry) {
                for (index, assembly) in cellular.definition.assemblies.iter().enumerate() {
                    if assembly.kind.surface_footprint() > 0.0 {
                        candidates.push((index, MutationKind::Geometry));
                    }
                }
            }
        }
        if candidates.is_empty() {
            return None;
        }
        let mut rng = MutationRng::new(seed);
        let (index, kind) = candidates[rng.index(candidates.len())];
        // Historical birth initializers sit outside the expression graph. Their
        // numeric token is still a constant mutation site, including a silent zero.
        if kind == MutationKind::Constant && index >= program.nodes.len() {
            let role = index - program.nodes.len();
            let literal = parsed.initializers[role]?;
            let old = source[literal.from..literal.to].parse::<f64>().ok()?;
            let Value::Scalar(changed) = mutate_constant(&Value::Scalar(old), &mut rng) else { return None; };
            let magnitude = changed.abs().clamp(0.0, MAX_SIGNAL);
            let magnitude = if magnitude == old { if old < 1.0 { old + 1.0 } else { old * 0.5 } } else { magnitude };
            let mut patched = source.to_string();
            patched.replace_range(literal.from..literal.to, &magnitude.to_string());
            return Some(Mutation { program: Self::compile_stored(&patched).ok()?, source: patched, kind });
        }
        if matches!(kind, MutationKind::Ingredient | MutationKind::Specificity | MutationKind::Turnover) {
            return Self::mutate_recipe(source, &parsed, index, kind, &mut rng);
        }
        if kind == MutationKind::Geometry {
            return Self::mutate_geometry(source, &parsed, index, &mut rng);
        }
        if matches!(
            kind,
            MutationKind::GeneSilence | MutationKind::GeneDuplicate
        ) {
            return Self::mutate_gene(source, &parsed, index, kind);
        }
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
                        Op::Peer(field) => field.name().to_string(),
                        Op::Pool(index) => {
                            format!("pool.{}", program.cellular().unwrap().products[*index].name)
                        }
                        Op::Concentration(index) => format!(
                            "concentration.{}",
                            program.cellular().unwrap().products[*index].name
                        ),
                        Op::Built(index) => format!(
                            "built.{}",
                            program.cellular().unwrap().assemblies[*index].name
                        ),
                        Op::Receptor(index, field) => format!(
                            "sensed.{}.{}",
                            program.cellular().unwrap().assemblies[*index].name,
                            field.name()
                        ),
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
            MutationKind::Any
            | MutationKind::GeneSilence
            | MutationKind::GeneDuplicate
            | MutationKind::Geometry
            | MutationKind::Ingredient
            | MutationKind::Specificity
            | MutationKind::Turnover => {
                unreachable!()
            }
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
        let compiled = Self::compile_stored(&patched).ok()?;
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

    fn mutate_recipe(
        source: &str, parsed: &ParsedSource, index: usize,
        kind: MutationKind, rng: &mut MutationRng,
    ) -> Option<Mutation> {
        let (span, alternatives) = if kind == MutationKind::Turnover {
            let product = &parsed.program.cellular()?.products[index];
            let span = parsed.lifetimes[index];
            let old = product.half_life;
            let changed = if old == 0.0 {
                (30 + rng.index(271)) as f64
            } else if product.kind != ProductKind::Regulator && rng.index(12) == 0 {
                0.0
            } else {
                let factor = 1.0 + (1 + rng.index(25)) as f64 / 100.0;
                let increase = old <= 0.001 || (old < 1.0e6 && rng.index(2) == 0);
                (if increase { old * factor } else { old / factor }).clamp(0.001, 1.0e6)
            };
            if source[span.from..span.to].contains('#') { return None; }
            let prefix = if span.from == span.to && product.kind == ProductKind::Adhesin { ", " } else { "" };
            (span, vec![format!("{prefix}{changed}")])
        } else if kind == MutationKind::Ingredient {
            let span = parsed.ingredients[index];
            let old = source[span.from..span.to].parse::<f64>().ok()?;
            let fraction = (1 + rng.index(25)) as f64 / 100.0;
            let step = if parsed.program.uses_count_units() {
                (old * fraction).round().max(1.0)
            } else {
                old * fraction
            };
            let sign = if rng.index(2) == 0 { 1.0 } else { -1.0 };
            let options = [old + sign * step, old - sign * step].into_iter()
                .filter(|v| v.is_finite() && *v > 0.0 && *v != old)
                .map(|v| v.to_string()).collect::<Vec<_>>();
            (span, options)
        } else {
            let recipe = &parsed.program.cellular()?.assemblies[index];
            let mut options: Vec<_> = specificity_options(recipe).map(str::to_owned).collect();
            let start = rng.index(options.len());
            options.rotate_left(start);
            (parsed.targets[index]?, options)
        };
        for replacement in alternatives {
            if source.len() - (span.to - span.from) + replacement.len() > MAX_SOURCE_BYTES {
                continue;
            }
            let mut patched = source.to_owned();
            patched.replace_range(span.from..span.to, &replacement);
            // Try the opposite dose or another target when physical validation
            // or a typed, target-specific receptor read rejects the first edit.
            if let Ok(program) = Self::compile_stored(&patched) {
                return Some(Mutation { program, source: patched, kind });
            }
        }
        None
    }

    fn mutate_geometry(
        source: &str,
        parsed: &ParsedSource,
        index: usize,
        rng: &mut MutationRng,
    ) -> Option<Mutation> {
        let assembly = &parsed.program.cellular()?.assemblies[index];
        let location = parsed.geometry[index];
        let direction = rng.next() & 1 != 0;
        let (field, previous, span) = if direction {
            ("direction", assembly.direction, location.direction)
        } else {
            ("at", assembly.at, location.at)
        };
        let signed = if rng.next() & 1 == 0 { -1.0 } else { 1.0 };
        let delta = signed * (0.05 + (rng.next() % 4501) as f64 / 10000.0);
        let changed = (previous + delta + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
            - std::f64::consts::PI;
        let (from, to, replacement) = if let Some(span) = span {
            // Preserve comments embedded between a minus sign and its literal.
            if source[span.from..span.to].contains('#') {
                return None;
            }
            (span.from, span.to, Value::Scalar(changed).source())
        } else {
            (
                location.close.from,
                location.close.from,
                format!("{field}: {};\n", Value::Scalar(changed).source()),
            )
        };
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
        let program = Self::compile_stored(&patched).ok()?;
        Some(Mutation {
            program,
            source: patched,
            kind: MutationKind::Geometry,
        })
    }

    fn mutate_gene(
        source: &str,
        parsed: &ParsedSource,
        index: usize,
        kind: MutationKind,
    ) -> Option<Mutation> {
        let cellular = parsed.program.cellular.as_ref()?;
        let location = parsed.genes[index];
        let (from, to, replacement) = if kind == MutationKind::GeneSilence {
            let span = location.expression;
            // Retain the inherited expression (and its comments) so a later
            // regulatory mutation can reactivate this locus.
            (
                span.from,
                span.to,
                format!("0 * ({})", &source[span.from..span.to]),
            )
        } else {
            let original_name = &cellular.genes[index].name;
            let base = &original_name[..original_name.len().min(52)];
            let mut serial = 2;
            let mut name = format!("{base}_{serial}");
            while cellular.genes.iter().any(|gene| gene.name == name) {
                serial += 1;
                name = format!("{base}_{serial}");
            }
            let span = location.declaration;
            let declaration = format!(
                "\n{}{}{}",
                &source[span.from..location.name.from],
                name,
                &source[location.name.to..span.to]
            );
            (span.to, span.to, declaration)
        };
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
        let program = Self::compile_stored(&patched).ok()?;
        Some(Mutation {
            program,
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
        let (per_contact,_) = contact_guard::input_dependencies(&program.nodes);
        let inspected = |idx: usize| if per_contact[idx] {
            "null,\"perContact\":true".to_owned()
        } else {
            values[idx].json()
        };
        let range = |from: usize, to: usize| {
            format!("\"from\":{},\"to\":{}", map.offsets[from], map.offsets[to])
        };
        let mut json = format!(
            "{{\"version\":1,\"mode\":{},\"cellId\":{cell_id},\"genomeId\":{genome_id},\"time\":{},\"tick\":{tick},\"semantics\":\"current-snapshot\",\"source\":{},\"expressions\":[",
            quote(program.mode()), inputs.time, quote(source),
        );
        for (visible, (index, node)) in program
            .nodes
            .iter()
            .enumerate()
            .filter(|(index, _)| parsed.nodes[*index].authored)
            .enumerate()
        {
            if visible > 0 {
                json.push(',');
            }
            let location = parsed.nodes[index];
            json.push_str(&format!(
                "{{\"id\":{index},{},\"type\":{},\"value\":{}}}",
                range(location.from, location.to),
                quote(node.ty.label()),
                inspected(index)
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
                inspected(binding.node)
            ));
        }
        json.push_str("],\"signals\":[");
        let mut signals = Sensor::ALL
            .iter()
            .filter(|sensor| program.cellular.is_none() || sensor.cellular_allowed()
                || (sensor.legacy_external() && program.reads_sensor(**sensor)))
            .map(|sensor| {
                (
                    sensor.name().to_string(),
                    sensor.program_type(program.uses_count_units()),
                    sensor.read_typed(inputs,sensor.program_type(program.uses_count_units())),
                )
            })
            .collect::<Vec<_>>();
        if let Some(cellular) = program.cellular() {
            let stock_type = if program.uses_count_units() { Type::Count } else { Type::Number };
            signals.extend(
                cellular
                    .products
                    .iter()
                    .enumerate()
                    .map(|(index, product)| {
                        (
                            format!("pool.{}", product.name),
                            stock_type,
                            evaluate_operation(&Op::Pool(index), stock_type, inputs, memory, &[]),
                        )
                    }),
            );
            signals.extend(
                cellular
                    .products
                    .iter()
                    .enumerate()
                    .map(|(index, product)| {
                        (
                            format!("concentration.{}", product.name),
                            Type::Number,
                            evaluate_operation(
                                &Op::Concentration(index),
                                Type::Number,
                                inputs,
                                memory,
                                &[],
                            ),
                        )
                    }),
            );
            signals.extend(
                cellular
                    .assemblies
                    .iter()
                    .enumerate()
                    .map(|(index, assembly)| {
                        (
                            format!("built.{}", assembly.name),
                            Type::Number,
                            evaluate_operation(
                                &Op::Built(index),
                                Type::Number,
                                inputs,
                                memory,
                                &[],
                            ),
                        )
                    }),
            );
            signals.extend(program.receptor_reads().iter().map(|&(index, field)| {
                (
                    format!(
                        "sensed.{}.{}",
                        cellular.assemblies[index].name,
                        field.name()
                    ),
                    field.ty(),
                    evaluate_operation(
                        &Op::Receptor(index, field),
                        field.ty(),
                        inputs,
                        memory,
                        &[],
                    ),
                )
            }));
        }
        for (index, (name, ty, value)) in signals.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "{{\"name\":{},\"type\":{},\"value\":{}}}",
                quote(name),
                quote(ty.label()),
                value.json()
            ));
        }
        json.push_str("],\"outputs\":[");
        for (index, root) in program
            .output_indices()
            .iter()
            .enumerate()
            .filter(|_| program.cellular.is_none())
        {
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
        if let Some(cellular) = &program.cellular {
            for (index, (name, root)) in cellular.named_roots().iter().enumerate() {
                if index > 0 {
                    json.push(',');
                }
                let per_contact = name.starts_with("engage_when.");
                let boolean = name.starts_with("start_when.") || per_contact;
                let ty = if boolean { "boolean" } else { "scalar" };
                let value = if per_contact {
                    "null".into()
                } else if boolean {
                    Value::Boolean(root.is_none_or(|root| values[root].boolean())).json()
                } else {
                    let default = if name.starts_with("recycle.") { 0.0 } else { 1.0 };
                    Value::Scalar(root.map_or(default, |root| values[root].number().clamp(0.0, 1.0))).json()
                };
                let offsets = root.map_or_else(
                    || "\"from\":null,\"to\":null".to_string(),
                    |root| {
                        let location = parsed.nodes[root];
                        range(location.from, location.to)
                    },
                );
                json.push_str(&format!(
                    "{{\"name\":{},\"type\":\"{ty}\",\"value\":{value},{}{offsets}}}",
                    quote(name), if per_contact { "\"perContact\":true," } else { "" }
                ));
            }
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
        json.push(']');
        if let Some(messages) = program.contact_messages() {
            json.push_str(&format!(",\"contactMessages\":{},\"contactMessageDeclarations\":[", messages.to_json()));
            let mut declarations = Vec::new();
            for (role, location) in parsed.messages.into_iter().enumerate() {
                if let (Some(index), Some(location)) = (messages.addresses[role], location) {
                    let value = messages.value(role, memory)
                        .map_or_else(|| "null".into(), |value| value.to_string());
                    declarations.push(format!("{{\"kind\":{},\"register\":\"rf{index:03}\",\"value\":{value},{}}}",
                        quote(cellular::ContactMessages::ROLES[role]), range(location.from, location.to)));
                }
            }
            json.push_str(&declarations.join(","));
            json.push(']');
        }
        json.push('}');
        Ok(json)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn composition_mutates_one_ingredient_in_either_unit_system() {
        for (header, dose) in [("cellular counts;", "800"), ("cellular;", "0.08")] {
            let source = format!("{header} product E=enzyme(); product S=scaffold();\r\nassembly Gut:digest(soluble){{ E:# dose\r\n{dose}; S:{dose}; }}");
            let original = Program::compile(&source).unwrap();
            let parent = &original.cellular().unwrap().assemblies[0];
            for seed in 0..24 {
                let mutation = Program::mutate_source(&source, seed, MutationKind::Ingredient).unwrap();
                let updated = &mutation.program.cellular().unwrap().assemblies[0];
                let changes: Vec<_> = parent.parts.iter().zip(&updated.parts).filter(|(a,b)| a != b).collect();
                assert_eq!(changes.len(), 1);
                assert_eq!(parent.kind, updated.kind);
                assert!(mutation.source.contains("# dose\r\n"));
                assert_eq!(mutation.source, Program::mutate_source(&source, seed, MutationKind::Ingredient).unwrap().source);
                let count = changes[0].1.1 * crate::counts::SCALE as f64;
                if original.uses_count_units() { assert!((count - count.round()).abs() < 1e-8); }
            }
        }
    }

    #[test]
    fn turnover_changes_one_product_and_preserves_its_precursor_and_comments() {
        let source = "cellular counts; product F = scaffold(# frame\n); product G=adhesin(F # glue\n); product E=enzyme(90); product R=regulator(2);";
        let parent = Program::compile(source).unwrap();
        let definition = parent.cellular().unwrap();
        let mut seen = HashSet::new();
        for seed in 0..128 {
            let m = Program::mutate_source(source, seed, MutationKind::Turnover).unwrap();
            let updated = m.program.cellular().unwrap();
            let changes: Vec<_> = updated.products.iter().zip(&definition.products)
                .filter(|(a,b)| a.half_life != b.half_life).collect();
            assert_eq!(changes.len(), 1);
            assert_eq!(updated.precursors, definition.precursors);
            assert!(m.source.contains("# frame\n") && m.source.contains("# glue\n"));
            assert!(updated.products.iter().all(|p| p.half_life >= 0.0));
            seen.insert(changes[0].0.name.clone());
        }
        assert_eq!(seen.len(), 4);
        assert!(Program::compile("cellular; product E=enzyme(-2);").is_err());
        assert!(Program::compile("cellular; product R=regulator(0);").is_err());
        // The new inventory has a qualified name so a preexisting declaration
        // named ligand and an actual inventory read can coexist.
        let old_name = "cellular counts; product ligand=enzyme(); gene make:ligand=if carbon.ligand > 0 then 1 else 0;";
        assert!(Program::compile(old_name).is_ok());
    }

    #[test]
    fn specificity_changes_only_one_substrate_and_can_arise_at_division() {
        let source = "cellular counts; product E=enzyme(); assembly Gut:digest( # binding site\r\nsoluble ){ E: 800; }";
        let replacement = Program::mutate_source(source, 12, MutationKind::Specificity).unwrap();
        let target = replacement.program.cellular().unwrap().assemblies[0].kind.name();
        assert!(matches!(target, "digest(structural)" | "digest(bound)"));
        let argument = target.strip_prefix("digest(").unwrap().trim_end_matches(')');
        assert_eq!(replacement.source, source.replace("soluble", argument));
        let variants: Vec<_> = (0..64).filter_map(|seed| Program::mutate_source(source, seed, MutationKind::Any)).collect();
        for kind in [MutationKind::Ingredient, MutationKind::Specificity] {
            assert!(variants.iter().any(|m| m.kind == kind));
        }
        assert!(Program::mutate_source("cellular; product E=enzyme(); assembly R:repair(){ E:0.1; }", 1, MutationKind::Specificity).is_none());
    }
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
