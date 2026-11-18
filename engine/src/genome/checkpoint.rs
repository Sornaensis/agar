use super::*;
use crate::checkpoint::{ensure, Reader, Result, Writer};

fn write_type(ty: Type, w: &mut Writer) {
    w.u8(match ty {
        Type::Integer => 0,
        Type::Number => 1,
        Type::Bool => 2,
        Type::String => 3,
        Type::Vector => 4,
    });
}
fn read_type(r: &mut Reader<'_>) -> Result<Type> {
    match r.u8()? {
        0 => Ok(Type::Integer),
        1 => Ok(Type::Number),
        2 => Ok(Type::Bool),
        3 => Ok(Type::String),
        4 => Ok(Type::Vector),
        _ => Err("Invalid simulation save: unknown primitive type.".into()),
    }
}
fn write_register(register: Register, w: &mut Writer) {
    write_type(register.ty, w);
    w.u16(register.index);
}
fn read_register(r: &mut Reader<'_>) -> Result<Register> {
    let ty = read_type(r)?;
    let index = r.u16()?;
    ensure(
        index > 0 && index <= REGISTER_COUNT as u16,
        "invalid register address",
    )?;
    Ok(Register { ty, index })
}
fn write_value(value: &Value, w: &mut Writer) {
    write_type(value.ty(), w);
    match value {
        Value::Integer(n) => w.i32(*n),
        Value::Scalar(n) => w.f64(*n),
        Value::Boolean(b) => w.u8(u8::from(*b)),
        Value::Text(s) => w.text(s),
        Value::Vector(v) => {
            w.f64(v[0]);
            w.f64(v[1]);
        }
    }
}
fn read_value(r: &mut Reader<'_>) -> Result<Value> {
    Ok(match read_type(r)? {
        Type::Integer => Value::Integer(r.i32()?),
        Type::Number => Value::Scalar(r.range(-MAX_SIGNAL, MAX_SIGNAL)?),
        Type::Bool => {
            let b = r.u8()?;
            ensure(b <= 1, "invalid boolean")?;
            Value::Boolean(b == 1)
        }
        Type::String => Value::Text(Rc::from(r.text(MAX_STRING_BYTES)?)),
        Type::Vector => Value::Vector([
            r.range(-MAX_SIGNAL, MAX_SIGNAL)?,
            r.range(-MAX_SIGNAL, MAX_SIGNAL)?,
        ]),
    })
}

impl Registers {
    pub(crate) fn write_checkpoint(&self, w: &mut Writer) {
        w.count(self.entries.len());
        for (register, value) in &self.entries {
            write_register(*register, w);
            write_value(value, w);
        }
    }
    pub(crate) fn read_checkpoint(r: &mut Reader<'_>) -> Result<Self> {
        let count = r.count(REGISTER_COUNT * 5)?;
        let mut entries = BTreeMap::new();
        for _ in 0..count {
            let register = read_register(r)?;
            let value = read_value(r)?;
            ensure(
                register.ty == value.ty(),
                "register value has the wrong type",
            )?;
            ensure(
                entries.insert(register, value).is_none(),
                "duplicate register address",
            )?;
        }
        Ok(Self { entries })
    }
}

impl Program {
    pub(crate) fn checkpoint_source_matches(
        &self,
        compiled: &Self,
        _source: &str,
        _authored: bool,
    ) -> bool {
        let mut a = Writer::new();
        let mut b = Writer::new();
        self.write_checkpoint(&mut a);
        compiled.write_checkpoint(&mut b);
        // Saves require the exact graph compiled from the stored source.
        // This also validates source-preserving mutations without canonicalization.
        !a.failed() && !b.failed() && a.0 == b.0
    }

    pub(crate) fn write_checkpoint(&self, w: &mut Writer) {
        if let Some(cellular) = &self.cellular {
            // An impossible legacy node count introduces the v5 extension.
            // Legacy programs retain their original byte representation.
            w.u32(u32::MAX);
            w.count(cellular.definition.products.len());
            for product in &cellular.definition.products {
                w.text(&product.name);
                w.text(product.kind.name());
                w.f64(product.half_life);
            }
            w.count(cellular.definition.assemblies.len());
            for assembly in &cellular.definition.assemblies {
                w.text(&assembly.name);
                w.text(assembly.kind.name());
                w.count(assembly.parts.len());
                for &(product, amount) in &assembly.parts {
                    w.count(product);
                    w.f64(amount);
                }
                w.f64(assembly.at);
                w.f64(assembly.direction);
            }
        }
        w.count(self.nodes.len());
        for node in &self.nodes {
            write_type(node.ty, w);
            w.count(node.scope);
            match &node.op {
                Op::Constant(value) => {
                    w.u8(0);
                    write_value(value, w);
                }
                Op::Sensor(sensor) => {
                    w.u8(1);
                    w.count(Sensor::ALL.iter().position(|s| s == sensor).unwrap());
                }
                Op::Pool(index) => {
                    w.u8(10);
                    w.count(*index);
                }
                Op::Built(index) => {
                    w.u8(11);
                    w.count(*index);
                }
                Op::Register(register) => {
                    w.u8(2);
                    write_register(*register, w);
                }
                Op::Populated(register) => {
                    w.u8(3);
                    write_register(*register, w);
                }
                Op::Reference(i) => {
                    w.u8(4);
                    w.count(*i);
                }
                Op::Negate(i) => {
                    w.u8(5);
                    w.count(*i);
                }
                Op::Not(i) => {
                    w.u8(6);
                    w.count(*i);
                }
                Op::Binary(op, a, b) => {
                    w.u8(7);
                    w.count(Binary::ALL.iter().position(|b| b == op).unwrap());
                    w.count(*a);
                    w.count(*b);
                }
                Op::Choose(a, b, c) => {
                    w.u8(8);
                    w.count(*a);
                    w.count(*b);
                    w.count(*c);
                }
                Op::Call(function, args) => {
                    w.u8(9);
                    w.count(Function::ALL.iter().position(|f| f == function).unwrap());
                    for arg in args {
                        w.count(*arg);
                    }
                }
            }
        }
        w.count(self.bindings.len());
        for binding in &self.bindings {
            w.text(&binding.name);
            w.count(binding.node);
        }
        for root in self.output_indices() {
            w.option(root);
        }
        w.count(self.writes.len());
        for rule in &self.writes {
            write_register(rule.register, w);
            w.option(rule.value);
            w.option(rule.guard);
        }
        if let Some(cellular) = &self.cellular {
            w.count(cellular.genes.len());
            for gene in &cellular.genes {
                w.text(&gene.name);
                w.count(gene.product);
                w.count(gene.root);
            }
            for root in &cellular.activity {
                w.option(*root);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn read_checkpoint(r: &mut Reader<'_>) -> Result<Self> {
        Self::read_checkpoint_version(r, 6)
    }

    pub(crate) fn read_checkpoint_version(r: &mut Reader<'_>, version: u32) -> Result<Self> {
        let header = r.u32()?;
        let cellular = if header == u32::MAX {
            ensure(version >= 5, "cellular programs need checkpoint version 5")?;
            let count = r.count(cellular::MAX_PRODUCTS)?;
            let mut products = Vec::with_capacity(count);
            for _ in 0..count {
                let name = r.text(64)?;
                ensure(cellular::valid_name(&name), "invalid product name")?;
                let kind = ProductKind::from_name(&r.text(64)?)
                    .ok_or("Invalid simulation save: unknown product kind.")?;
                let half_life = r.range(0.0, 1.0e6)?;
                ensure(
                    if kind == ProductKind::Regulator {
                        half_life >= 0.001
                    } else {
                        half_life == 0.0
                    },
                    "invalid product half-life",
                )?;
                products.push(ProductDef {
                    name,
                    kind,
                    half_life,
                });
            }
            let count = r.count(cellular::MAX_ASSEMBLIES)?;
            let mut assemblies = Vec::with_capacity(count);
            for _ in 0..count {
                let name = r.text(64)?;
                ensure(cellular::valid_name(&name), "invalid assembly name")?;
                let kind = AssemblyKind::from_name(&r.text(64)?)
                    .ok_or("Invalid simulation save: unknown assembly kind.")?;
                ensure(
                    version >= 6 || !matches!(kind, AssemblyKind::Mechanism(_)),
                    "mechanism assemblies need checkpoint version 6",
                )?;
                let count = r.count(cellular::MAX_PRODUCTS)?;
                let mut parts = Vec::with_capacity(count);
                for _ in 0..count {
                    parts.push((r.index(products.len())?, r.range(0.0, 1.0e6)?));
                }
                let at = r.range(-MAX_SIGNAL, MAX_SIGNAL)?;
                let direction = r.range(-MAX_SIGNAL, MAX_SIGNAL)?;
                assemblies.push(AssemblyDef {
                    name,
                    kind,
                    parts,
                    at,
                    direction,
                });
            }
            let definition = CellularProgram {
                products,
                assemblies,
            };
            definition
                .validate()
                .map_err(|error| format!("Invalid simulation save: {error}"))?;
            Some(definition)
        } else {
            None
        };
        let count = if cellular.is_some() {
            r.count(MAX_NODES)?
        } else {
            header as usize
        };
        ensure(count <= MAX_NODES, "collection exceeds its limit")?;
        ensure(count > 0 || cellular.is_some(), "genome has no expressions")?;
        let mut nodes: Vec<Node> = Vec::with_capacity(count);
        for index in 0..count {
            let ty = read_type(r)?;
            let scope = r.count(MAX_NODES)?;
            let op = match r.u8()? {
                0 => Op::Constant(read_value(r)?),
                1 => Op::Sensor(
                    Sensor::ALL[r.index(if version == 3 { 144 } else { Sensor::ALL.len() })?],
                ),
                2 => Op::Register(read_register(r)?),
                3 => Op::Populated(read_register(r)?),
                4 => Op::Reference(r.index(index)?),
                5 => Op::Negate(r.index(index)?),
                6 => Op::Not(r.index(index)?),
                7 => Op::Binary(
                    Binary::ALL[r.index(Binary::ALL.len())?],
                    r.index(index)?,
                    r.index(index)?,
                ),
                8 => Op::Choose(r.index(index)?, r.index(index)?, r.index(index)?),
                9 => {
                    let function = Function::ALL[r.index(Function::ALL.len())?];
                    let mut args = [0; 3];
                    for (i, arg) in args.iter_mut().enumerate() {
                        *arg = if i < function.arity() {
                            r.index(index)?
                        } else {
                            r.count(MAX_NODES)?
                        };
                    }
                    Op::Call(function, args)
                }
                10 if cellular.is_some() => {
                    Op::Pool(r.index(cellular.as_ref().unwrap().products.len())?)
                }
                11 if cellular.is_some() => {
                    Op::Built(r.index(cellular.as_ref().unwrap().assemblies.len())?)
                }
                _ => return Err("Invalid simulation save: unknown expression operation.".into()),
            };
            let child_type = |child: usize| nodes[child].ty;
            let actual_type = match &op {
                Op::Constant(value) => Some(value.ty()),
                Op::Sensor(sensor) => Some(sensor.ty()),
                Op::Pool(_) | Op::Built(_) => Some(Type::Number),
                Op::Register(register) => Some(register.ty),
                Op::Populated(_) => Some(Type::Bool),
                Op::Reference(child) => Some(child_type(*child)),
                Op::Negate(child) => {
                    let t = child_type(*child);
                    (t.numeric() || t == Type::Vector).then_some(t)
                }
                Op::Not(child) => (child_type(*child) == Type::Bool).then_some(Type::Bool),
                Op::Binary(op, a, b) => binary_type(*op, child_type(*a), child_type(*b)),
                Op::Choose(c, a, b) => {
                    if child_type(*c) != Type::Bool {
                        None
                    } else if child_type(*a) == child_type(*b) {
                        Some(child_type(*a))
                    } else {
                        numeric_type(&[child_type(*a), child_type(*b)])
                    }
                }
                Op::Call(f, args) => f.ty(&args[..f.arity()]
                    .iter()
                    .map(|i| child_type(*i))
                    .collect::<Vec<_>>()),
            };
            if cellular.is_some() {
                ensure(
                    match &op {
                        Op::Sensor(sensor) => sensor.cellular_allowed(),
                        Op::Register(_) | Op::Populated(_) => false,
                        _ => true,
                    },
                    "legacy signal in cellular program",
                )?;
            }
            ensure(actual_type == Some(ty), "expression type mismatch")?;
            nodes.push(Node { op, ty, scope });
        }
        let count = r.count(MAX_NODES)?;
        let mut bindings = Vec::with_capacity(count);
        let mut names = HashSet::new();
        for _ in 0..count {
            let name = r.text(MAX_SOURCE_BYTES)?;
            let node = r.index(nodes.len())?;
            if cellular.is_some() {
                ensure(cellular::valid_name(&name), "invalid cellular binding name")?;
            }
            ensure(
                !name.is_empty()
                    && name.bytes().enumerate().all(|(i, c)| {
                        c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit())
                    })
                    && !Register::reserved(&name)
                    && output_index(&name).is_none()
                    && Sensor::lookup(&name).is_none()
                    && Function::lookup(&name).is_none()
                    && !matches!(
                        name.as_str(),
                        "if" | "then"
                            | "else"
                            | "let"
                            | "true"
                            | "false"
                            | "when"
                            | "clear"
                            | "populated"
                            | "food"
                            | "neighbor"
                            | "prey"
                            | "threat"
                            | "flow"
                            | "light"
                            | "memory"
                            | "temperature"
                    )
                    && names.insert(name.clone()),
                "invalid binding name",
            )?;
            bindings.push(Binding { name, node });
        }
        for (index, node) in nodes.iter().enumerate() {
            ensure(
                node.scope <= bindings.len()
                    && bindings[..node.scope].iter().all(|b| b.node < index),
                "invalid lexical scope",
            )?;
        }
        let mut roots = [None; OUTPUT_NAMES.len()];
        let output_count = if version == 3 {
            DIGEST_OUTPUT_START
        } else {
            OUTPUT_COUNT
        };
        for (index, root) in roots.iter_mut().enumerate().take(output_count) {
            *root = r.option(nodes.len())?;
            if let Some(id) = *root {
                ensure(
                    nodes[id].ty == output_type(index)
                        || (output_type(index) == Type::Number && nodes[id].ty == Type::Integer),
                    "output type mismatch",
                )?;
            }
        }
        ensure(
            cellular.is_some() || roots[..3].iter().all(Option::is_some),
            "required output is missing",
        )?;
        let count = r.count(REGISTER_COUNT * 5 * 2)?;
        let mut writes = Vec::with_capacity(count);
        let mut destinations = HashSet::new();
        for _ in 0..count {
            let register = read_register(r)?;
            let value = r.option(nodes.len())?;
            let guard = r.option(nodes.len())?;
            if let Some(id) = value {
                ensure(
                    nodes[id].ty == register.ty
                        || (register.ty == Type::Number && nodes[id].ty == Type::Integer),
                    "write type mismatch",
                )?;
            }
            if let Some(id) = guard {
                ensure(nodes[id].ty == Type::Bool, "write condition is not boolean")?;
            }
            ensure(
                destinations.insert((register, value.is_some())),
                "duplicate register rule",
            )?;
            writes.push(WriteRule {
                register,
                value,
                guard,
            });
        }
        if let Some(definition) = cellular {
            ensure(
                roots.iter().all(Option::is_none) && writes.is_empty(),
                "legacy actions in cellular program",
            )?;
            let count = r.count(cellular::MAX_GENES)?;
            let mut genes = Vec::with_capacity(count);
            let mut names = HashSet::new();
            for _ in 0..count {
                let name = r.text(64)?;
                ensure(
                    cellular::valid_name(&name) && names.insert(name.clone()),
                    "invalid gene name",
                )?;
                let product = r.index(definition.products.len())?;
                let root = r.index(nodes.len())?;
                ensure(nodes[root].ty.numeric(), "gene expression is not numeric")?;
                genes.push(cellular::GeneRoot {
                    name,
                    product,
                    root,
                });
            }
            let mut activity = Vec::with_capacity(definition.assemblies.len());
            for _ in &definition.assemblies {
                let root = r.option(nodes.len())?;
                if let Some(root) = root {
                    ensure(nodes[root].ty.numeric(), "assembly activity is not numeric")?;
                }
                activity.push(root);
            }
            return Ok(Self::from_cellular(
                nodes,
                bindings,
                cellular::CellularGraph {
                    definition,
                    genes,
                    activity,
                },
            ));
        }
        Ok(Self::from_parts(nodes, bindings, roots, writes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_retains_typed_body_and_scan_outputs_and_exact_mutated_source() {
        let source = "thrust=0;turn=0;divide=false;target.optical.count=2;target.optical.size=.04;build.optical=.7;scan.target=object.token;scan.width=3.14;";
        let program = Program::compile(source).unwrap();
        let mut w = Writer::new();
        program.write_checkpoint(&mut w);
        let restored = Program::read_checkpoint(&mut Reader::new(&w.0)).unwrap();
        let inputs = Inputs {
            object_token: Rc::from("track:5"),
            ..Inputs::default()
        };
        assert_eq!(restored.evaluate(&inputs), program.evaluate(&inputs));
        assert!(restored.checkpoint_source_matches(
            &Program::compile(source).unwrap(),
            source,
            true
        ));
        assert_eq!(restored.output_roots.len(), OUTPUT_COUNT);
        for (root_index, replacement_index) in [
            (BODY_OUTPUT_START + 15 * 4, SCAN_OUTPUT_START + 1),
            (SCAN_OUTPUT_START + 2, BODY_OUTPUT_START + 15 * 4),
        ] {
            let mut invalid = program.clone();
            invalid.output_roots[root_index] = invalid.output_roots[replacement_index];
            let mut bad = Writer::new();
            invalid.write_checkpoint(&mut bad);
            assert!(Program::read_checkpoint(&mut Reader::new(&bad.0)).is_err());
        }
        for seed in 1..80 {
            let mutation = Program::mutate_source(source, seed, MutationKind::Any).unwrap();
            let mut w = Writer::new();
            mutation.program.write_checkpoint(&mut w);
            let saved = Program::read_checkpoint(&mut Reader::new(&w.0)).unwrap();
            let compiled = Program::compile(&mutation.source).unwrap();
            assert!(saved.checkpoint_source_matches(&compiled, &mutation.source, false));
            assert_eq!(saved.evaluate(&inputs), compiled.evaluate(&inputs));
        }
        let other = Program::compile(&source.replace("count=2", "count=3")).unwrap();
        assert!(!program.checkpoint_source_matches(&other, source, false));
    }

    #[test]
    fn decoder_rejects_invalid_types_cycles_names_and_register_occupancy() {
        let original = Program::compile(
            "let fuel = energy; ri001 = 1; thrust = fuel; turn = 0; divide = false;",
        )
        .unwrap();
        for variation in 0..4 {
            let mut program = original.clone();
            match variation {
                0 => program.nodes[0].op = Op::Reference(0),
                1 => program.nodes[0].ty = Type::Bool,
                2 => program.bindings[0].name = "if".into(),
                _ => program.writes.push(program.writes[0].clone()),
            }
            let mut w = Writer::new();
            program.write_checkpoint(&mut w);
            assert!(Program::read_checkpoint(&mut Reader::new(&w.0)).is_err());
        }
        let key = Register::parse("ri001").unwrap();
        let mut memory = Registers::default();
        memory.entries.insert(key, Value::Boolean(false));
        let mut w = Writer::new();
        memory.write_checkpoint(&mut w);
        assert!(Registers::read_checkpoint(&mut Reader::new(&w.0)).is_err());
    }
}
