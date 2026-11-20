use super::*;
use crate::checkpoint::{ensure, Reader, Result, Writer};

fn write_type(ty: Type, w: &mut Writer) {
    w.u8(match ty {
        Type::Integer => 0,
        Type::Number => 1,
        Type::Bool => 2,
        Type::String => 3,
        Type::Vector => 4,
        Type::Count => 5,
    });
}
fn read_type(r: &mut Reader<'_>) -> Result<Type> {
    match r.u8()? {
        0 => Ok(Type::Integer),
        1 => Ok(Type::Number),
        2 => Ok(Type::Bool),
        3 => Ok(Type::String),
        4 => Ok(Type::Vector),
        5 => Ok(Type::Count),
        _ => Err("Invalid simulation save: unknown primitive type.".into()),
    }
}
fn write_register(register: Register, w: &mut Writer) {
    write_type(register.ty, w);
    w.u16(register.index);
}
fn read_register(r: &mut Reader<'_>) -> Result<Register> {
    let ty = read_type(r)?;
    ensure(ty != Type::Count, "invalid register type")?;
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
        Value::Count(n) => w.u32(*n),
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
        Type::Count => Value::Count(r.u32()?),
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
            if !cellular.contact_roots().is_empty() { w.u32(u32::MAX - 6); }
            if cellular.count_units { w.u32(u32::MAX - 5); }
            // An impossible legacy node count introduces the v5 extension.
            // Legacy programs retain their original byte representation.
            if let Some(messages) = &cellular.messages {
                // Preserve the old representation for programs without contact
                // declarations. The new sentinel cannot be a bounded node count.
                let has_signal = messages.addresses[3].is_some();
                let tag = match (messages.recruits_contacts, has_signal) {
                    (false, false) => 1, (false, true) => 2, (true, false) => 3, (true, true) => 4,
                };
                w.u32(u32::MAX - tag);
                let slots = if has_signal { 4 } else { 3 };
                for index in &messages.addresses[..slots] { w.u16(index.unwrap_or(0)); }
                let initializers = [messages.initial, messages.signal_initial];
                for value in &initializers[..if has_signal { 2 } else { 1 }] {
                    w.u8(value.is_some() as u8);
                    if let Some(value) = value { w.f64(*value); }
                }
            } else {
                w.u32(u32::MAX);
            }
            w.count(cellular.definition.products.len());
            for (index, product) in cellular.definition.products.iter().enumerate() {
                w.text(&product.name);
                w.text(product.kind.name());
                w.f64(product.half_life);
                if product.kind == ProductKind::Adhesin {
                    // Only new product records carry an input index, keeping old graphs byte-identical.
                    w.u32(cellular.definition.precursors.get(index).copied().flatten()
                        .and_then(|i| u32::try_from(i).ok()).unwrap_or(u32::MAX));
                }
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
                Op::Concentration(index) => {
                    w.u8(12);
                    w.count(*index);
                }
                Op::Built(index) => {
                    w.u8(11);
                    w.count(*index);
                }
                Op::Receptor(index, field) => {
                    w.u8(13);
                    w.count(*index);
                    w.u8(*field as u8);
                }
                Op::Peer(field) => {
                    w.u8(14);
                    w.u8(*field as u8);
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
            for root in &cellular.start {
                w.option(*root);
            }
            for root in &cellular.recycle {
                w.option(*root);
            }
            if !cellular.contact_roots().is_empty() {
                w.count(cellular.contact_roots().len());
                for &(recipe,root) in cellular.contact_roots() { w.count(recipe); w.count(root); }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn read_checkpoint(r: &mut Reader<'_>) -> Result<Self> {
        Self::read_checkpoint_version(r, crate::checkpoint::VERSION)
    }

    pub(crate) fn read_checkpoint_version(r: &mut Reader<'_>, version: u32) -> Result<Self> {
        let mut header = r.u32()?;
        let guarded = header == u32::MAX - 6;
        if guarded {
            ensure(version >= 19, "contact guards need checkpoint version 19")?;
            header = r.u32()?;
            ensure(header >= u32::MAX - 5, "invalid guarded cellular program header")?;
        }
        let count_units = header == u32::MAX - 5;
        if count_units {
            ensure(version >= 17, "counted cellular programs need checkpoint version 17")?;
            header = r.u32()?;
            ensure(header >= u32::MAX - 4, "invalid counted cellular graph header")?;
        }
        let recruits = header == u32::MAX - 3 || header == u32::MAX - 4;
        let has_signal = header == u32::MAX - 2 || header == u32::MAX - 4;
        let messages = if header == u32::MAX - 1 || has_signal || recruits {
            ensure(!recruits || version >= 15, "contact recruitment needs checkpoint version 15")?;
            ensure(version >= 10, "contact declarations need checkpoint version 10")?;
            ensure(!has_signal || version >= 11, "contact payloads need checkpoint version 11")?;
            let mut messages = cellular::ContactMessages::default();
            let slots = if has_signal { 4 } else { 3 };
            for slot in &mut messages.addresses[..slots] {
                let index = r.u16()?;
                ensure(index as usize <= REGISTER_COUNT, "invalid scalar contact register")?;
                *slot = (index != 0).then_some(index);
            }
            ensure(messages.addresses.iter().any(Option::is_some), "missing contact declaration")?;
            ensure(!recruits || messages.addresses[2].is_some(), "recruitment needs an adhesion register")?;
            ensure(version < 19 || messages.addresses[1].is_none() && messages.addresses[2].is_none() && !recruits,
                "obsolete whole-cell contact policy")?;
            messages.recruits_contacts = recruits;
            if has_signal {
                ensure(messages.addresses[3].is_some() && messages.addresses[3] != messages.addresses[0],
                    "payload requires an independent scalar register")?;
            }
            let mut read_initial = |address: Option<u16>| -> Result<Option<f64>> {
                let flag = r.u8()?;
                ensure(flag <= 1, "invalid message initializer flag")?;
                if flag == 0 { return Ok(None); }
                ensure(address.is_some(), "initializer without published register")?;
                Ok(Some(r.range(-MAX_SIGNAL, MAX_SIGNAL)?))
            };
            messages.initial = read_initial(messages.addresses[0])?;
            if has_signal { messages.signal_initial = read_initial(messages.addresses[3])?; }
            Some(Box::new(messages))
        } else { None };
        let cellular = if header == u32::MAX || messages.is_some() {
            ensure(version >= 5, "cellular programs need checkpoint version 5")?;
            let count = r.count(cellular::MAX_PRODUCTS)?;
            let mut products = Vec::with_capacity(count);
            let mut precursors = Vec::new();
            for index in 0..count {
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
                if kind == ProductKind::Adhesin {
                    ensure(version >= 16, "adhesin synthesis needs checkpoint version 16")?;
                    if precursors.is_empty() { precursors = vec![None; count]; }
                    precursors[index] = Some(r.index(count)?);
                }
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
                ensure(version >= 12 || kind != AssemblyKind::Mechanism(crate::cellular::MechanismKind::JunctionSignal),
                    "signal junctions need checkpoint version 12")?;
                ensure(version >= 11 || !matches!(kind, AssemblyKind::Mechanism(k) if k as usize >= 27),
                    "junction machinery needs checkpoint version 11")?;
                ensure(version >= 10 || kind.name() != "adhere()", "adhesion needs checkpoint version 10")?;
                ensure(
                    version >= 9 || kind.name() != "respire(oxygen)",
                    "aerobic respiration needs checkpoint version 9",
                )?;
                ensure(
                    version >= 7 || kind.name() != "recycle()",
                    "recycling needs checkpoint version 7",
                )?;
                let count = r.count(cellular::MAX_PRODUCTS)?;
                let mut parts = Vec::with_capacity(count);
                for _ in 0..count {
                    let product = r.index(products.len())?;
                    let amount = r.range(0.0, 1.0e6)?;
                    if count_units {
                        let count = resource_count(amount);
                        ensure(count > 0 && count as f64 / crate::counts::SCALE as f64 == amount,
                            "invalid counted recipe ingredient")?;
                    }
                    parts.push((product, amount));
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
            let mut definition = CellularProgram::new(products, assemblies);
            definition.precursors = precursors;
            definition
                .validate_stored()
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
            ensure(ty != Type::Count || count_units, "count type requires a counted program")?;
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
                    let function = Function::ALL
                        [r.index(if version < 7 { 21 } else { Function::ALL.len() })?];
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
                12 if version >= 7 && cellular.is_some() => {
                    Op::Concentration(r.index(cellular.as_ref().unwrap().products.len())?)
                }
                13 if version >= 7 && cellular.is_some() => {
                    let definition = cellular.as_ref().unwrap();
                    let index = r.index(definition.assemblies.len())?;
                    let field = ReceptorField::ALL
                        .get(r.u8()? as usize)
                        .copied()
                        .ok_or("Invalid simulation save: unknown receptor field.")?;
                    ensure(version >= 12 || field != ReceptorField::Count, "junction count needs checkpoint version 12")?;
                    ensure(version >= 15 || field != ReceptorField::Bonded, "bond sensing needs checkpoint version 15")?;
                    ensure(version >= 11 || !matches!(field, ReceptorField::Signal | ReceptorField::SignalValid),
                        "contact payloads need checkpoint version 11")?;
                    ensure(version >= 10 || !matches!(field, ReceptorField::Message | ReceptorField::MessageValid),
                        "contact messages need checkpoint version 10")?;
                    ensure(
                        ReceptorField::for_kind(definition.assemblies[index].kind).contains(&field),
                        "receptor does not supply that measurement",
                    )?;
                    Op::Receptor(index, field)
                }
                14 if version >= 19 && cellular.is_some() => {
                    Op::Peer(*PeerField::ALL.get(r.u8()? as usize).ok_or("Invalid simulation save: unknown contact field.")?)
                }
                _ => return Err("Invalid simulation save: unknown expression operation.".into()),
            };
            let child_type = |child: usize| nodes[child].ty;
            let actual_type = match &op {
                Op::Constant(value) => Some(value.ty()),
                Op::Sensor(sensor) => Some(sensor.program_type(count_units)),
                Op::Pool(_) if count_units => Some(Type::Count),
                Op::Pool(_) | Op::Concentration(_) | Op::Built(_) => Some(Type::Number),
                Op::Receptor(_, field) => Some(field.ty()),
                Op::Peer(field) => Some(field.ty()),
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
                        Op::Sensor(sensor) => sensor.cellular_allowed()
                            && (version >= 10 || !matches!(sensor, Sensor::MemoryUsed | Sensor::MemoryCost)),
                        Op::Register(_) | Op::Populated(_) => version >= 10,
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
                        || (register.ty == Type::Number && nodes[id].ty.numeric())
                        || (register.ty == Type::Integer && nodes[id].ty == Type::Count),
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
                roots.iter().all(Option::is_none) && (version >= 10 || writes.is_empty()),
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
            let mut start = vec![None; definition.assemblies.len()];
            let mut recycle = vec![None; definition.assemblies.len()];
            if version >= 7 {
                for slot in &mut start {
                    *slot = r.option(nodes.len())?;
                    if let Some(root) = *slot {
                        ensure(
                            nodes[root].ty == Type::Bool,
                            "assembly start condition is not boolean",
                        )?;
                    }
                }
                for slot in &mut recycle {
                    *slot = r.option(nodes.len())?;
                    if let Some(root) = *slot {
                        ensure(
                            nodes[root].ty.numeric(),
                            "assembly recycling is not numeric",
                        )?;
                    }
                }
            }
            let mut guards = Vec::new();
            if guarded {
                let count = r.count(definition.assemblies.len())?;
                ensure(count > 0, "empty contact guard set")?;
                for _ in 0..count {
                    let recipe = r.index(definition.assemblies.len())?;
                    let root = r.index(nodes.len())?;
                    ensure(contact_guard::supports_guard(definition.assemblies[recipe].kind) && nodes[root].ty==Type::Bool
                        && guards.last().is_none_or(|&(previous,_)| previous < recipe), "invalid contact guard")?;
                    guards.push((recipe,root));
                }
            }
            let local_roots = genes.iter().map(|g|g.root).chain(activity.iter().chain(&start).chain(&recycle).flatten().copied())
                .chain(writes.iter().flat_map(|w|w.value.into_iter().chain(w.guard)));
            contact_guard::validate_contexts(&nodes,&guards,local_roots)
                .map_err(|(_, message)| format!("Invalid simulation save: {message}"))?;
            let engagement = contact_guard::ContactGuards::compile(&nodes, guards, &definition);
            return Ok(Self::from_cellular(
                nodes,
                bindings,
                writes,
                cellular::CellularGraph {
                    count_units,
                    definition,
                    genes,
                    activity,
                    start,
                    recycle,
                    messages,
                    engagement,
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
