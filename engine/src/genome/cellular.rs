//! Cellular declarations share the pure expression graph with the legacy mode.
//! Products and recipes are typed physical definitions; only gene regulation and
//! assembly activity are expressions. This keeps a numerical mutation from
//! silently changing the chemical identity of material already inside a cell.
use super::*;
use crate::cellular::MechanismKind;

pub(super) const MAX_PRODUCTS: usize = 64;
pub(super) const MAX_ASSEMBLIES: usize = 64;
pub(super) const MAX_GENES: usize = 256;

#[derive(Debug, Clone)]
pub(super) struct GeneRoot {
    pub name: String,
    pub product: usize,
    pub root: usize,
}

/// A paid receptor can observe the identity and payload of the same touched
/// cell. The receiver’s avoidance and adhesion comparisons are private.
#[derive(Debug, Clone, Default)]
pub(super) struct ContactMessages {
    // Publish identity, avoid, adhere, publish signal. Slots are scalar register addresses.
    pub addresses: [Option<u16>; 4],
    pub initial: Option<f64>,
    pub signal_initial: Option<f64>,
}
impl ContactMessages {
    pub(super) const ROLES: [&'static str; 4] = ["publish", "avoid", "adhere", "signal"];

    /// The v10 budget stored the three declaration slots and one initializer.
    /// Charge added representation overhead at runtime for old declarations.
    pub(super) fn legacy_layout_overhead(&self) -> usize {
        if self.addresses[3].is_none() {
            std::mem::size_of::<Self>() - std::mem::size_of::<([Option<u16>; 3], Option<f64>)>()
        } else { 0 }
    }
    pub(super) fn value(&self, role: usize, memory: &Registers) -> Option<f64> {
        let register = Register { ty: Type::Number, index: self.addresses[role]? };
        match memory.entries.get(&register) {
            Some(Value::Scalar(value)) => Some(*value),
            _ => None,
        }
    }
    pub(super) fn to_json(&self) -> String {
        let mut fields = Self::ROLES.iter().zip(self.addresses).map(|(role, index)| {
            let address = index.map_or_else(|| "null".into(), |index| quote(&format!("rf{index:03}")));
            format!("\"{role}\":{address}")
        }).collect::<Vec<_>>();
        fields.push(format!("\"initial\":{}", self.initial.map_or_else(|| "null".into(), |v| v.to_string())));
        fields.push(format!("\"signalInitial\":{}", self.signal_initial.map_or_else(|| "null".into(), |v| v.to_string())));
        format!("{{{}}}", fields.join(","))
    }
}

#[derive(Debug, Clone)]
pub(super) struct CellularGraph {
    pub definition: CellularProgram,
    pub genes: Vec<GeneRoot>,
    pub activity: Vec<Option<usize>>,
    pub start: Vec<Option<usize>>,
    pub recycle: Vec<Option<usize>>,
    pub messages: Option<Box<ContactMessages>>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct GeneLocation {
    pub declaration: Location,
    pub name: Location,
    pub expression: Location,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct GeometryLocation {
    pub close: Location,
    pub at: Option<Location>,
    pub direction: Option<Location>,
}

impl CellularGraph {
    pub(super) fn roots(&self) -> Vec<usize> {
        self.genes
            .iter()
            .map(|gene| gene.root)
            .chain(self.activity.iter().flatten().copied())
            .chain(self.start.iter().flatten().copied())
            .chain(self.recycle.iter().flatten().copied())
            .collect()
    }

    pub(super) fn allocated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            - std::mem::size_of::<CellularProgram>() + CellularProgram::authored_inline_bytes()
            + self.definition.products.capacity() * std::mem::size_of::<ProductDef>()
            + self.definition.assemblies.capacity() * std::mem::size_of::<AssemblyDef>()
            + self.genes.capacity() * std::mem::size_of::<GeneRoot>()
            + self.activity.capacity() * std::mem::size_of::<Option<usize>>()
            + self.start.capacity() * std::mem::size_of::<Option<usize>>()
            + self.recycle.capacity() * std::mem::size_of::<Option<usize>>()
            + self
                .definition
                .products
                .iter()
                .map(|p| p.name.capacity())
                .sum::<usize>()
            + self
                .definition
                .assemblies
                .iter()
                .map(|a| {
                    a.name.capacity() + a.parts.capacity() * std::mem::size_of::<(usize, f64)>()
                })
                .sum::<usize>()
            + self.genes.iter().map(|g| g.name.capacity()).sum::<usize>()
            + self.messages.as_ref().map_or(0, |_| std::mem::size_of::<ContactMessages>())
            + self.definition.derived_bytes()
    }

    pub(super) fn controls(&self, roots: &[usize], values: &[Value]) -> CellularControls {
        let mut result = CellularControls::default();
        self.controls_into(roots, values, &mut result);
        result
    }

    pub(super) fn controls_into(
        &self,
        roots: &[usize],
        values: &[Value],
        result: &mut CellularControls,
    ) {
        result
            .expression
            .resize(self.definition.products.len(), 0.0);
        result.expression.fill(0.0);
        for (gene, &root) in self.genes.iter().zip(roots) {
            // Each gene has a bounded effort, while gene duplication changes
            // its share of the cell's finite synthesis budget.
            result.expression[gene.product] += values[root].number().clamp(0.0, 1.0);
        }
        let mut next = self.genes.len();
        result.activity.clear();
        for root in &self.activity {
            result.activity.push(if root.is_some() {
                let value = values[roots[next]].number().clamp(0.0, 1.0);
                next += 1;
                value
            } else {
                1.0
            });
        }
        result.start.clear();
        for root in &self.start {
            result.start.push(if root.is_some() {
                let value = values[roots[next]].boolean();
                next += 1;
                value
            } else {
                true
            });
        }
        result.recycle.clear();
        for root in &self.recycle {
            result.recycle.push(if root.is_some() {
                let value = values[roots[next]].number().clamp(0.0, 1.0);
                next += 1;
                value
            } else {
                0.0
            });
        }
    }

    pub(super) fn named_roots(&self) -> Vec<(String, Option<usize>)> {
        self.genes
            .iter()
            .map(|gene| (format!("gene.{}", gene.name), Some(gene.root)))
            .chain(
                self.definition
                    .assemblies
                    .iter()
                    .zip(&self.activity)
                    .map(|(assembly, root)| (format!("activity.{}", assembly.name), *root)),
            )
            .chain(
                self.definition
                    .assemblies
                    .iter()
                    .zip(&self.start)
                    .map(|(assembly, root)| (format!("start_when.{}", assembly.name), *root)),
            )
            .chain(
                self.definition
                    .assemblies
                    .iter()
                    .zip(&self.recycle)
                    .map(|(assembly, root)| (format!("recycle.{}", assembly.name), *root)),
            )
            .collect()
    }

    pub(super) fn graph_roots_json(&self) -> String {
        self.named_roots().iter().map(|(name, root)| {
            let value = root.map_or_else(|| "null".to_string(), |i| i.to_string());
            let ty = if name.starts_with("start_when.") { "boolean" } else { "scalar" };
            let default = if root.is_some() { "null" } else if name.starts_with("start_when.") { "\"true\"" }
                else if name.starts_with("recycle.") { "\"0\"" } else { "\"1\"" };
            format!("{{\"id\":{},\"kind\":\"output\",\"name\":{},\"type\":\"{ty}\",\"value\":{value},\"guard\":null,\"default\":{default}}}",
                quote(&format!("output:{name}")), quote(name))
        }).collect::<Vec<_>>().join(",")
    }
}

impl Sensor {
    pub(super) fn cellular_allowed(self) -> bool {
        let name = self.name();
        matches!(
            name,
            "energy" | "energy.fraction" | "energy.capacity" | "age" | "speed" | "integrity"
                | "oxygen" | "respiration.rate" | "memory.used" | "memory.cost"
        ) || (name.starts_with("body.") && name != "body.division_ready")
            || [
                "nutrient.",
                "carbon.",
                "gut.",
                "light.",
                "heat.",
                "chemical.",
                "flow.",
                "contact.",
                "object.",
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix))
    }
}

impl Program {
    /// Additive v7 authored metadata, used only to migrate earlier registry
    /// memory budgets. The old source and physical recipes retain their meaning.
    pub(crate) fn cellular_control_overhead_bytes(&self) -> usize {
        self.cellular.as_ref().map_or(0, |graph| {
            2 * std::mem::size_of::<Vec<Option<usize>>>()
                + (graph.start.capacity() + graph.recycle.capacity())
                    * std::mem::size_of::<Option<usize>>()
        })
    }

    pub(super) fn contact_messages(&self) -> Option<&ContactMessages> {
        self.cellular.as_ref()?.messages.as_deref()
    }
    pub fn participates_in_contact_signaling(&self) -> bool {
        self.contact_messages().is_some()
    }
    pub fn published_message(&self, memory: &Registers) -> Option<f64> {
        self.contact_messages()?.value(0, memory)
    }
    pub fn avoid_message(&self, memory: &Registers) -> Option<f64> {
        self.contact_messages()?.value(1, memory)
    }
    pub fn adhesion_message(&self, memory: &Registers) -> Option<f64> {
        self.contact_messages()?.value(2, memory)
    }
    pub fn contact_signal(&self, memory: &Registers) -> Option<f64> {
        self.contact_messages()?.value(3, memory)
    }
    pub fn message_initializer(&self) -> Option<f64> {
        self.contact_messages()?.initial
    }
    /// Seed a newly created cell, including a daughter carrying a mutated
    /// program. Loading a checkpoint must preserve an explicitly cleared slot.
    pub fn initialize_message(&self, memory: &mut Registers) {
        if let Some(messages) = self.contact_messages() {
            for (index, value) in [(messages.addresses[0], messages.initial),
                                   (messages.addresses[3], messages.signal_initial)] {
                if let (Some(index), Some(value)) = (index, value) {
                    memory.entries.insert(Register { ty: Type::Number, index }, Value::Scalar(value));
                }
            }
        }
    }

    pub fn receptor_reads(&self) -> &[(usize, ReceptorField)] {
        &self.execution.receptor_reads
    }
    pub fn cellular(&self) -> Option<&CellularProgram> {
        self.cellular.as_ref().map(|graph| &graph.definition)
    }

    pub fn mode(&self) -> &'static str {
        if self.cellular.is_some() {
            "cellular"
        } else {
            "legacy"
        }
    }

    pub(super) fn from_cellular(
        nodes: Vec<Node>,
        bindings: Vec<Binding>,
        writes: Vec<WriteRule>,
        graph: CellularGraph,
    ) -> Self {
        let output_roots = [None; OUTPUT_COUNT];
        let execution = ExecutionPlan::compile(&nodes, &output_roots, &writes, &graph.roots());
        Self {
            nodes,
            bindings,
            output_roots,
            writes,
            execution,
            cellular: Some(Box::new(graph)),
        }
    }

    pub(super) fn evaluated_outputs(&self, values: &[Value]) -> Outputs {
        if let Some(cellular) = &self.cellular {
            Outputs {
                cellular: Some(cellular.controls(&self.execution.extra_roots, values)),
                ..Outputs::default()
            }
        } else {
            Self::outputs_for(&self.execution.output_roots, values)
        }
    }

    pub(super) fn evaluated_outputs_into(&self, values: &[Value], output: &mut Outputs) {
        if let Some(cellular) = &self.cellular {
            let mut controls = output.cellular.take().unwrap_or_default();
            cellular.controls_into(&self.execution.extra_roots, values, &mut controls);
            *output = Outputs {
                cellular: Some(controls),
                ..Outputs::default()
            };
        } else {
            *output = Self::outputs_for(&self.execution.output_roots, values);
        }
    }
}

pub(super) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .enumerate()
            .all(|(i, c)| c.is_ascii_alphabetic() || c == b'_' || (i > 0 && c.is_ascii_digit()))
        && !matches!(
            name,
            "cellular"
                | "product"
                | "gene"
                | "assembly"
                | "activity"
                | "at"
                | "direction"
                | "let"
                | "if"
                | "then"
                | "else"
                | "true"
                | "false"
                | "when"
                | "clear"
                | "populated"
                | "pool"
                | "built"
                | "food"
                | "neighbor"
                | "prey"
                | "threat"
                | "flow"
                | "light"
                | "memory"
                | "temperature"
        )
        && !Register::reserved(name)
        && Sensor::lookup(name).is_none()
        && Function::lookup(name).is_none()
        && output_index(name).is_none()
        && ProductKind::from_name(name).is_none()
        && AssemblyKind::from_name(name).is_none()
}

impl Parser {
    fn declaration_name(&mut self, what: &str) -> Result<(String, Location), CompileError> {
        let token = self.take();
        let Kind::Name(name) = token.kind else {
            return Err(token.location.error(format!("expected a {what} name.")));
        };
        if !valid_name(&name) {
            return Err(token.location.error(format!("'{name}' is reserved or not a valid {what} name; use at most 64 letters, digits, or underscores.")));
        }
        Ok((name, token.location))
    }

    // Product/assembly names are declarations, not temporal values. Discovering
    // them first allows a gene to read built.Swimmer before its recipe appears.
    fn cellular_names(&mut self) -> Result<(), CompileError> {
        let mut depth = 0usize;
        for (i, token) in self.tokens.iter().enumerate() {
            match &token.kind {
                Kind::LeftBrace => depth += 1,
                Kind::RightBrace => depth = depth.saturating_sub(1),
                Kind::Name(keyword)
                    if depth == 0 && matches!(keyword.as_str(), "product" | "assembly") =>
                {
                    let Some(Token {
                        kind: Kind::Name(name),
                        location,
                    }) = self.tokens.get(i + 1)
                    else {
                        continue;
                    };
                    let (names, limit) = if keyword == "product" {
                        (&mut self.product_names, MAX_PRODUCTS)
                    } else {
                        (&mut self.assembly_names, MAX_ASSEMBLIES)
                    };
                    if names.contains_key(name) {
                        return Err(
                            location.error(format!("{keyword} '{name}' is already defined."))
                        );
                    }
                    if names.len() >= limit {
                        return Err(location.error(format!(
                            "cellular programs are limited to {limit} {keyword} declarations."
                        )));
                    }
                    names.insert(name.clone(), names.len());
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn material_number(&mut self) -> Result<f64, CompileError> {
        let negative = if self.peek().kind == Kind::Minus {
            self.take();
            true
        } else {
            false
        };
        let token = self.take();
        let value =
            match token.kind {
                Kind::Integer(value) => value as f64,
                Kind::Number(value) => value,
                _ => return Err(token.location.error(
                    "expected a numeric literal; physical definitions cannot contain expressions.",
                )),
            };
        Ok(if negative { -value } else { value })
    }

    fn assembly_kind(&mut self) -> Result<AssemblyKind, CompileError> {
        let token = self.take();
        let Kind::Name(constructor) = &token.kind else {
            return Err(token
                .location
                .error("expected a physical mechanism such as store(food) or translate()."));
        };
        if self.peek().kind != Kind::LeftParen {
            // Bare family names retain their v5 semantics. Constructor names
            // are contextual: adding a mechanism must not invalidate an old
            // product or gene whose identifier happens to have the same name.
            return AssemblyKind::from_name(constructor).ok_or_else(|| {
                token.location.error(format!(
                    "unknown assembly kind '{constructor}'; mechanisms use constructor syntax, such as translate()."
                ))
            });
        }
        self.take();
        let target = if self.peek().kind == Kind::RightParen {
            None
        } else {
            let argument = self.take();
            let Kind::Name(target) = argument.kind else {
                return Err(argument.location.error(
                    "mechanism targets must be names such as food, mineral, or tissue, not strings or expressions.",
                ));
            };
            Some(target)
        };
        self.expect(Kind::RightParen, "')' after the mechanism target")?;
        let name = format!("{constructor}({})", target.as_deref().unwrap_or(""));
        MechanismKind::from_name(&name)
            .map(AssemblyKind::Mechanism)
            .ok_or_else(|| {
                let allowed = MechanismKind::ALL
                    .iter()
                    .filter(|kind| kind.constructor() == constructor)
                    .map(|kind| kind.name())
                    .collect::<Vec<_>>();
                let explanation = if allowed.is_empty() {
                    format!("unknown mechanism constructor '{constructor}'.")
                } else {
                    format!(
                        "unsupported mechanism '{name}'; use {}.",
                        allowed.join(" or ")
                    )
                };
                token.location.error(explanation)
            })
    }

    pub(super) fn cellular_program(&mut self) -> Result<Program, CompileError> {
        self.cellular_mode = true;
        self.keyword("cellular")?;
        self.expect(Kind::Semicolon, "';' after cellular")?;
        self.cellular_names()?;
        let mut products = Vec::new();
        let mut assemblies = Vec::new();
        let mut activity = Vec::new();
        let mut start = Vec::new();
        let mut recycle = Vec::new();
        let mut assembly_locations = Vec::new();
        let mut genes = Vec::new();
        let mut gene_names = HashSet::new();
        let mut writes = Vec::new();
        let mut register_rules = HashSet::new();
        let mut messages = ContactMessages::default();
        while self.peek().kind != Kind::End {
            let declaration = self.take();
            let Kind::Name(keyword) = &declaration.kind else {
                return Err(declaration
                    .location
                    .error("expected product, gene, assembly, fn, or let declaration."));
            };
            match keyword.as_str() {
                "product" => {
                    let (name, location) = self.declaration_name("product")?;
                    self.expect(Kind::Assign, "'=' after product name")?;
                    let constructor = self.take();
                    let Kind::Name(kind_name) = &constructor.kind else {
                        return Err(constructor.location.error("expected a typed product constructor, such as motor()."));
                    };
                    let kind = ProductKind::from_name(kind_name).ok_or_else(|| constructor.location.error(format!("unknown product constructor '{kind_name}'.")))?;
                    self.expect(Kind::LeftParen, "'(' after product constructor")?;
                    let half_life = if kind == ProductKind::Regulator {
                        let value = self.material_number()?;
                        if !(0.001..=1.0e6).contains(&value) {
                            return Err(constructor.location.error("regulator half-life must be between 0.001 and 1000000 seconds."));
                        }
                        value
                    } else { 0.0 };
                    self.expect(Kind::RightParen, "')' after product constructor arguments")?;
                    self.expect(Kind::Semicolon, "';' after product declaration")?;
                    if self.product_names.get(&name) != Some(&products.len()) {
                        return Err(location.error("product declaration order is invalid."));
                    }
                    products.push(ProductDef { name, kind, half_life });
                }
                "gene" => {
                    let (name, name_location) = self.declaration_name("gene")?;
                    if !gene_names.insert(name.clone()) { return Err(name_location.error(format!("gene '{name}' is already defined."))); }
                    if genes.len() >= MAX_GENES { return Err(name_location.error(format!("cellular programs are limited to {MAX_GENES} genes."))); }
                    self.expect(Kind::Colon, "':' after gene name")?;
                    let target = self.take();
                    let Kind::Name(product_name) = &target.kind else { return Err(target.location.error("expected the product made by this gene.")); };
                    let product = self.product_names.get(product_name).copied().ok_or_else(|| target.location.error(format!("unknown product '{product_name}'.")))?;
                    self.expect(Kind::Assign, "'=' before gene expression")?;
                    let expression_start = self.peek().location;
                    let root = self.expression(0, 0)?;
                    self.assignable(root, Type::Number, expression_start)?;
                    let expression_end = self.tokens[self.position.saturating_sub(1)].location.to;
                    self.expect(Kind::Semicolon, "';' after gene expression")?;
                    self.gene_locations.push(GeneLocation {
                        declaration: declaration.location.through(self.tokens[self.position - 1].location.to),
                        name: name_location,
                        expression: expression_start.through(expression_end),
                    });
                    genes.push(GeneRoot { name, product, root });
                }
                "assembly" => {
                    let (name, location) = self.declaration_name("assembly")?;
                    self.expect(Kind::Colon, "':' after assembly name")?;
                    let kind = self.assembly_kind()?;
                    self.expect(Kind::LeftBrace, "'{' before assembly recipe")?;
                    let mut parts = Vec::new();
                    let mut fields = HashSet::new();
                    let (mut at, mut direction, mut active) = (0.0, 0.0, None);
                    let (mut start_root, mut recycle_root) = (None, None);
                    let (mut at_location, mut direction_location) = (None, None);
                    while !matches!(self.peek().kind, Kind::RightBrace | Kind::End) {
                        let field = self.take();
                        let Kind::Name(field_name) = &field.kind else { return Err(field.location.error("expected a product name, activity, at, or direction.")); };
                        if !fields.insert(field_name.clone()) { return Err(field.location.error(format!("assembly field '{field_name}' is already defined."))); }
                        self.expect(Kind::Colon, "':' after assembly field")?;
                        match field_name.as_str() {
                            "activity" => {
                                let root = self.expression(0, 0)?;
                                self.assignable(root, Type::Number, field.location)?;
                                active = Some(root);
                            }
                            "start_when" => {
                                let root = self.expression(0, 0)?;
                                self.require(root, Type::Bool, field.location)?;
                                start_root = Some(root);
                            }
                            "recycle" => {
                                let root = self.expression(0, 0)?;
                                self.assignable(root, Type::Number, field.location)?;
                                recycle_root = Some(root);
                            }
                            "at" | "direction" => {
                                let value_start = self.peek().location;
                                let value = self.material_number()?;
                                let value_span = value_start.through(self.tokens[self.position - 1].location.to);
                                if value.abs() > MAX_SIGNAL { return Err(field.location.error("assembly angles exceed the numeric limit.")); }
                                if field_name == "at" { at = value; at_location = Some(value_span); }
                                else { direction = value; direction_location = Some(value_span); }
                            }
                            _ => {
                                let product = self.product_names.get(field_name).copied().ok_or_else(|| field.location.error(format!("unknown recipe product '{field_name}'.")))?;
                                let amount = self.material_number()?;
                                if !(0.0..=1.0e6).contains(&amount) || amount == 0.0 {
                                    return Err(field.location.error("recipe material amounts must be positive and no more than 1000000."));
                                }
                                parts.push((product, amount));
                            }
                        }
                        self.expect(Kind::Semicolon, "';' after assembly field")?;
                    }
                    let close = self.peek().location;
                    self.expect(Kind::RightBrace, "'}' after assembly recipe")?;
                    if self.peek().kind == Kind::Semicolon { self.take(); }
                    if self.assembly_names.get(&name) != Some(&assemblies.len()) { return Err(location.error("assembly declaration order is invalid.")); }
                    assemblies.push(AssemblyDef { name, kind, parts, at, direction });
                    assembly_locations.push(location);
                    activity.push(active);
                    start.push(start_root);
                    recycle.push(recycle_root);
                    self.geometry_locations.push(GeometryLocation { close, at: at_location, direction: direction_location });
                }
                "publish" | "avoid_message" | "adhere_message" | "signal" => {
                    let role = match keyword.as_str() { "publish" => 0, "avoid_message" => 1, "adhere_message" => 2, _ => 3 };
                    let token = self.take();
                    let Kind::Name(name) = &token.kind else {
                        return Err(token.location.error("expected a scalar register rf001 through rf999."));
                    };
                    let register = Self::register(name, token.location)?;
                    if register.ty != Type::Number {
                        return Err(token.location.error("contact messages require a scalar register rf001 through rf999."));
                    }
                    if messages.addresses[role].replace(register.index).is_some() {
                        return Err(declaration.location.error(format!("duplicate '{keyword}' declaration.")));
                    }
                    if messages.addresses[0].is_some() && messages.addresses[0] == messages.addresses[3] {
                        return Err(token.location.error("publish and signal require different scalar register addresses."));
                    }
                    if matches!(role, 0 | 3) && self.peek().kind == Kind::Assign {
                        self.take();
                        let location = self.peek().location;
                        let initial = self.material_number()?;
                        if !initial.is_finite() || initial.abs() > MAX_SIGNAL {
                            return Err(location.error("message initializer exceeds the scalar limit."));
                        }
                        if role == 0 { messages.initial = Some(initial); }
                        else { messages.signal_initial = Some(initial); }
                    }
                    self.expect(Kind::Semicolon, "';' after contact-message declaration")?;
                    self.message_locations[role] = Some(declaration.location.through(self.tokens[self.position-1].location.to));
                }
                "clear" => {
                    let token = self.take();
                    let Kind::Name(name) = &token.kind else {
                        return Err(token.location.error("expected a register after 'clear'."));
                    };
                    let register = Self::register(name, token.location)?;
                    if !register_rules.insert((register, false)) {
                        return Err(token.location.error(format!("register '{name}' already has a clear rule.")));
                    }
                    let guard = self.guard()?;
                    self.expect(Kind::Semicolon, "';' after clear")?;
                    self.write_locations.push(token.location);
                    writes.push(WriteRule { register, value: None, guard });
                }
                name if Register::reserved(name) => {
                    let register = Self::register(name, declaration.location)?;
                    if !register_rules.insert((register, true)) {
                        return Err(declaration.location.error(format!("register '{name}' already has a write rule.")));
                    }
                    self.expect(Kind::Assign, "'=' after register address")?;
                    let value = self.expression(0, 0)?;
                    self.assignable(value, register.ty, declaration.location)?;
                    let guard = self.guard()?;
                    self.expect(Kind::Semicolon, "';' after register write")?;
                    self.write_locations.push(declaration.location);
                    writes.push(WriteRule { register, value: Some(value), guard });
                }
                "fn" => self.function_declaration(declaration.location)?,
                "let" => {
                    let (name, location) = self.declaration_name("signal")?;
                    if self.bindings.contains_key(&name) || self.functions.contains_key(&name) { return Err(location.error(format!("signal '{name}' is already defined; bindings are immutable."))); }
                    self.expect(Kind::Assign, "'=' after signal name")?;
                    let node = self.expression(0, 0)?;
                    self.expect(Kind::Semicolon, "';' after signal expression")?;
                    self.binding_order.push(Binding { name: name.clone(), node });
                    self.binding_locations.push(location);
                    self.bindings.insert(name, node);
                }
                _ => return Err(declaration.location.error(format!("'{keyword}' is unavailable in cellular mode; express products with genes and assemble physical machinery."))),
            }
        }
        let definition = CellularProgram::new(
            products,
            assemblies,
        );
        for &(index, field, location) in &self.receptor_references {
            if !ReceptorField::for_kind(definition.assemblies[index].kind).contains(&field) {
                return Err(location.error(format!(
                    "assembly '{}' does not measure '{}'.",
                    definition.assemblies[index].name,
                    field.name()
                )));
            }
        }
        for (assembly, location) in definition.assemblies.iter().zip(assembly_locations) {
            // Report recipe failures at their declaration, not at end-of-file.
            let single = CellularProgram::new(
                definition.products.clone(),
                vec![assembly.clone()],
            );
            single.validate().map_err(|error| location.error(error))?;
        }
        definition
            .validate()
            .map_err(|error| self.peek().location.error(error))?;
        Ok(Program::from_cellular(
            std::mem::take(&mut self.nodes),
            std::mem::take(&mut self.binding_order),
            writes,
            CellularGraph {
                definition,
                genes,
                activity,
                start,
                recycle,
                messages: messages.addresses.iter().any(Option::is_some).then(|| Box::new(messages)),
            },
        ))
    }
}

pub(super) fn schema_json() -> String {
    let products = [
        "scaffold",
        "anchor",
        "motor",
        "filament",
        "pigment",
        "enzyme",
        "membrane",
        "transporter",
        "receptor",
        "regulator",
        "replicase",
        "constrictor",
    ]
    .iter()
    .map(|name| {
        format!(
            "{{\"name\":{},\"arguments\":[{}]}}",
            quote(name),
            if *name == "regulator" {
                "\"halfLife\""
            } else {
                ""
            }
        )
    })
    .collect::<Vec<_>>()
    .join(",");
    let mechanisms = MechanismKind::ALL
        .iter()
        .map(|kind| {
            let roles = kind
                .required_roles()
                .iter()
                .map(|role| quote(role.name()))
                .collect::<Vec<_>>()
                .join(",");
            let fields = ReceptorField::for_kind(AssemblyKind::Mechanism(*kind)).iter().map(|field| {
                format!("{{\"name\":{},\"type\":{},\"requiresValid\":{}}}", quote(field.name()), quote(field.ty().label()),
                    !matches!(field, ReceptorField::Available | ReceptorField::Valid | ReceptorField::Flow | ReceptorField::Speed | ReceptorField::Bearing))
            }).collect::<Vec<_>>().join(",");
            format!(
                "{{\"name\":{},\"constructor\":{},\"target\":{},\"requiredRoles\":[{roles}],\"description\":{},\"receptorFields\":[{fields}]}}",
                quote(kind.name()),
                quote(kind.constructor()),
                kind.target().map_or_else(|| "null".into(), quote),
                quote(kind.description()),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let assemblies = [
        "core",
        "photosynthetic",
        "metabolic",
        "digestive",
        "reserve",
        "transporter",
        "phagocyte",
        "flagellum",
        "piercer",
        "cortex",
        "repair",
        "photoreceptor",
        "thermoreceptor",
        "chemoreceptor",
        "mechanoreceptor",
        "optical",
        "membrane",
        "replicator",
        "constrictor",
    ]
    .iter()
    .map(|name| {
        let kind = AssemblyKind::from_name(name).expect("built-in assembly kind");
        let roles = kind
            .required_roles()
            .iter()
            .map(|role| quote(role.name()))
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"name\":{},\"requiredRoles\":[{roles}]}}", quote(name))
    })
    .collect::<Vec<_>>()
    .join(",");
    let reads = Sensor::ALL
        .iter()
        .filter(|s| s.cellular_allowed())
        .map(|s| {
            format!(
                "{{\"name\":{},\"type\":{}}}",
                quote(s.name()),
                quote(s.ty().label())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"marker\":\"cellular;\",\"products\":[{products}],\"assemblies\":[{assemblies}],\"mechanisms\":[{mechanisms}],\"reads\":[{reads}],\"maxProducts\":{MAX_PRODUCTS},\"maxAssemblies\":{MAX_ASSEMBLIES},\"maxGenes\":{MAX_GENES},\"registers\":{{\"prefixes\":[\"ri\",\"rf\",\"rb\",\"rs\",\"rv\"],\"addressesPerType\":{REGISTER_COUNT},\"upkeepPerSecond\":{REGISTER_ENERGY_COST},\"clearWins\":true}},\"contactMessages\":{{\"declarations\":[\"publish\",\"avoid_message\",\"adhere_message\",\"signal\"],\"registerType\":\"scalar\",\"receptor\":\"sense(contact)\",\"validField\":\"message_valid\",\"valueField\":\"message\",\"signalValidField\":\"signal_valid\",\"signalValueField\":\"signal\",\"initializerLimit\":{MAX_SIGNAL}}},\"recipeControls\":[{{\"name\":\"activity\",\"type\":\"scalar\",\"default\":1}},{{\"name\":\"start_when\",\"type\":\"boolean\",\"default\":true}},{{\"name\":\"recycle\",\"type\":\"scalar\",\"default\":0}}],\"concentration\":\"pool mass / body.area\",\"userFunctions\":{{\"keyword\":\"fn\",\"maxParameters\":8,\"parameterType\":\"scalar\",\"maxFunctions\":64,\"recursive\":false}}}}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::{Reader, Writer};

    const SWIMMER: &str = r#"# A physical flagellum, including paid-for regulatory memory.
cellular;
product Motor = motor();
product Anchor = anchor();
product Tail = filament();
product Run = regulator(8);
let ready = energy > 20 && nutrient.stored > 0.01;
gene drive: Motor = if ready && built.Swim < 0.08 && pool.Motor < 0.02 then 1 else 0;
gene mount: Anchor = if pool.Anchor < 0.01 then 0.5 else 0;
gene tail: Tail = if pool.Tail < 0.04 then 1 else 0;
gene latch: Run = if chemical.valid && chemical.mineral > 0.2 then 0.4 else 0;
assembly Swim: flagellum {
  Anchor: 0.005;
  Motor: 0.01;
  Tail: 0.025;
  activity: clamp(pool.Run * 100, 0, 1);
  at: 1.5707963267948966;
  direction: 0;
}
"#;

    fn inputs() -> Inputs {
        Inputs {
            energy: 50.0,
            nutrient_stored: 0.05,
            chemical_valid: true,
            chemical_mineral: 0.8,
            products: vec![0.0, 0.0, 0.0, 0.003],
            assemblies: vec![0.04],
            ..Inputs::default()
        }
    }

    #[test]
    fn typed_products_recipes_and_forward_mass_reads_compile() {
        let program = Program::compile(SWIMMER).unwrap();
        assert_eq!(program.mode(), "cellular");
        let definition = program.cellular().unwrap();
        assert_eq!(definition.products.len(), 4);
        assert_eq!(definition.products[3].kind, ProductKind::Regulator);
        assert_eq!(definition.products[3].half_life, 8.0);
        assert_eq!(
            definition.assemblies[0].parts,
            [(1, 0.005), (0, 0.01), (2, 0.025)]
        );
        assert_eq!(definition.assemblies[0].at, std::f64::consts::FRAC_PI_2);
        let output = program.evaluate(&inputs());
        let controls = output.cellular.unwrap();
        assert_eq!(controls.expression, [1.0, 0.5, 1.0, 0.4]);
        assert_eq!(controls.activity, [0.3]);
        assert_eq!(output.thrust, 0.0);
        assert_eq!(output.grow, 0.0);
        assert!(!output.divide);
        assert!(program.writes.is_empty());
        assert!(program.output_roots.iter().all(Option::is_none));
    }

    #[test]
    fn cellular_reads_are_a_pure_snapshot_and_missing_stocks_are_zero() {
        let source = "cellular; product Pulse = regulator(4); gene first: Pulse = 1; gene second: Pulse = pool.Pulse;";
        let program = Program::compile(source).unwrap();
        let mut memory = Registers::default();
        let mut scratch = EvalScratch::default();
        for _ in 0..3 {
            let result = program.evaluate_cell(&Inputs::default(), &mut memory, &mut scratch);
            assert_eq!(result.cellular.unwrap().expression, [1.0]);
            assert_eq!(memory.populated_count(), 0);
        }
        let inputs = Inputs {
            products: vec![0.25],
            ..Inputs::default()
        };
        assert_eq!(
            program.evaluate(&inputs).cellular.unwrap().expression,
            [1.25]
        );
        assert_eq!(inputs.products, [0.25]);
    }

    #[test]
    fn compact_execution_matches_authored_graph_and_does_not_fold_mass_reads() {
        let source = format!("{}\nlet unused = sqrt(energy) + pool.Motor;", SWIMMER);
        let program = Program::compile(&source).unwrap();
        for stock in [0.0, 0.001, 0.02, 0.5] {
            for installed in [0.0, 0.04, 0.08, 1.0] {
                let mut inputs = inputs();
                inputs.products[0] = stock;
                inputs.products[3] = stock;
                inputs.assemblies[0] = installed;
                let mut values = Vec::new();
                program.evaluate_nodes(&inputs, &Registers::default(), &mut values);
                let graph = program.cellular.as_ref().unwrap();
                let authored = graph.controls(&graph.roots(), &values);
                assert_eq!(program.evaluate(&inputs).cellular.unwrap(), authored);
            }
        }
        assert!(program.reachable().iter().any(|reached| !reached));
        assert!(!program.execution.extra_roots.is_empty());
    }

    #[test]
    fn cellular_mode_rejects_legacy_actions_and_unavailable_measurements() {
        for statement in [
            "thrust = 1;",
            "target.flagellum.count = 4;",
            "express.core = 1;",
        ] {
            let error = Program::compile(&format!("cellular; {statement}")).unwrap_err();
            assert!(error.contains("unavailable in cellular mode"), "{error}");
        }
        for expression in [
            "protein.flagellum",
            "organelle.core.count",
            "body.division_ready",
            "time",
            "id",
        ] {
            assert!(
                Program::compile(&format!(
                    "cellular; product A = motor(); gene make: A = {expression};"
                ))
                .is_err(),
                "{expression}"
            );
        }
        assert!(Program::compile("thrust=0;turn=0;divide=false;rf001=energy;").is_ok());
    }

    #[test]
    fn cellular_registers_read_the_prior_tick_and_only_publish_the_declared_scalar() {
        let source = r#"cellular;
# 🌿
publish rf001 = 0;
avoid_message rf002;
adhere_message rf001;
fn increment(x) = x + rf001;
ri001 = ri001 + 1;
rf001 = increment(1) when ri001 < 2;
rf002 = rf001;
rb001 = populated(rf001);
rs001 = "private";
rv001 = vec(rf001, rf002);
clear rf001 when ri001 == 1;
"#;
        let p = Program::compile(source).unwrap();
        let mut memory = Registers::default();
        assert_eq!(p.message_initializer(), Some(0.0));
        assert_eq!(p.published_message(&memory), None);
        p.initialize_message(&mut memory);
        assert_eq!(p.published_message(&memory), Some(0.0));
        assert_eq!(p.avoid_message(&memory), None);
        assert_eq!(p.adhesion_message(&memory), Some(0.0));
        let inputs = Inputs::default();
        let mut scratch = EvalScratch::default();
        p.evaluate_cell(&inputs, &mut memory, &mut scratch);
        assert_eq!(p.published_message(&memory), Some(1.0));
        assert_eq!(p.avoid_message(&memory), Some(0.0));
        assert_eq!(memory.value("rb001"), Some(&Value::Boolean(true)));
        assert_eq!(memory.value("rs001"), Some(&Value::Text(Rc::from("private"))));
        assert_eq!(memory.value("rv001"), Some(&Value::Vector([0.0, 0.0])));
        let before = memory.clone();
        let trace = Program::evaluation_json(source, &inputs, &memory, 1, 1, 1).unwrap();
        assert_eq!(memory, before);
        assert!(trace.contains(r#""kind":"clear","type":"scalar","value":null,"guard":true,"applies":true"#));
        let from = source[..source.find("publish").unwrap()].encode_utf16().count();
        let to = from + "publish rf001 = 0;".len();
        assert!(trace.contains(&format!(r#"{{"kind":"publish","register":"rf001","value":1,"from":{from},"to":{to}}}"#)));
        assert!(p.graph_json().contains(r#""roots":[{"id":"write:ri001""#));
        assert!(p.graph_json().contains(r#""contactMessages":{"publish":"rf001","avoid":"rf002","adhere":"rf001","signal":null,"initial":0,"signalInitial":null}"#));
        p.evaluate_cell(&inputs, &mut memory, &mut scratch);
        assert_eq!(p.published_message(&memory), None);
        assert_eq!(p.adhesion_message(&memory), None);
        assert_eq!(p.avoid_message(&memory), Some(1.0));
        assert_eq!(memory.value("rv001"), Some(&Value::Vector([1.0, 0.0])));
        let mut w = Writer::new();
        p.write_checkpoint(&mut w);
        assert!(Program::read_checkpoint_version(&mut Reader::new(&w.0), 9).is_err());
        let restored = Program::read_checkpoint(&mut Reader::new(&w.0)).unwrap();
        assert!(restored.checkpoint_source_matches(&p, source, true));
        let mut w = Writer::new();
        memory.write_checkpoint(&mut w);
        let mut resumed = Registers::read_checkpoint(&mut Reader::new(&w.0)).unwrap();
        assert_eq!(restored.published_message(&resumed), None);
        for _ in 0..5 {
            p.evaluate_cell(&inputs, &mut memory, &mut scratch);
            restored.evaluate_cell(&inputs, &mut resumed, &mut scratch);
            assert_eq!(memory, resumed);
        }
    }

    #[test]
    fn independent_payload_keeps_identity_and_checked_source_spans_through_replay() {
        let source = "cellular; # 🌱\npublish rf001=17; signal rf002 = 0; avoid_message rf001; adhere_message rf001; rf002=rf002-0.5; clear rf002 when ri001==1; ri001=ri001+1;";
        let program = Program::compile(source).unwrap();
        let mut memory = Registers::default();
        program.initialize_message(&mut memory);
        assert_eq!(program.published_message(&memory),Some(17.0));
        assert_eq!(program.contact_signal(&memory),Some(0.0));
        let trace = Program::evaluation_json(source,&Inputs::default(),&memory,1,1,0).unwrap();
        let from = source[..source.find("signal rf002").unwrap()].encode_utf16().count();
        let to = from + "signal rf002 = 0;".len();
        assert!(trace.contains(&format!(r#"{{"kind":"signal","register":"rf002","value":0,"from":{from},"to":{to}}}"#)));
        let mut scratch = EvalScratch::default();
        program.evaluate_cell(&Inputs::default(),&mut memory,&mut scratch);
        assert_eq!(program.contact_signal(&memory),Some(-0.5));
        program.evaluate_cell(&Inputs::default(),&mut memory,&mut scratch);
        assert_eq!(program.contact_signal(&memory), None);
        assert_eq!(program.avoid_message(&memory), Some(17.0));
        assert_eq!(program.adhesion_message(&memory), Some(17.0));
        let mut writer = Writer::new();
        program.write_checkpoint(&mut writer);
        assert!(Program::read_checkpoint_version(&mut Reader::new(&writer.0),10).is_err());
        let restored = Program::read_checkpoint(&mut Reader::new(&writer.0)).unwrap();
        assert!(restored.checkpoint_source_matches(&program, source, true));
        assert_eq!(restored.contact_signal(&memory),None);
        let payload_only = Program::compile("cellular; signal rf999 = -2;").unwrap();
        memory = Registers::default();
        payload_only.initialize_message(&mut memory);
        assert_eq!(payload_only.contact_signal(&memory),Some(-2.0));
        assert_eq!(payload_only.published_message(&memory),None);
        // All the old identifier contexts remain usable after adding this keyword.
        for old in ["cellular; let signal=1; rf001=signal;", "cellular; fn signal(x)=x; rf001=signal(1);",
            "cellular; product signal=anchor(); gene g:signal=1;"] {
            let p = Program::compile(old).unwrap();
            let mut writer = Writer::new();
            p.write_checkpoint(&mut writer);
            let old_p = Program::read_checkpoint_version(&mut Reader::new(&writer.0),10).unwrap();
            assert!(old_p.checkpoint_source_matches(&p,old,true));
        }
        for bad in ["signal ri001;", "signal rf001=energy;", "signal rf001=1e13;",
            "signal rf001; signal rf002;", "publish rf001; signal rf001;",
            "signal rf001; publish rf001;", "signal rf001 when true;"] {
            assert!(Program::compile(&format!("cellular; {bad}")).is_err(), "{bad}");
        }
    }

    #[test]
    fn checkpoint_gates_payload_reads_even_without_a_local_publisher() {
        let touch = "cellular; product A=anchor(); product R=receptor(); product F=filament(); assembly Touch:sense(contact){A:.01;R:.01;F:.01;}";
        for field in ["pressure", "message", "message_valid", "signal", "signal_valid"] {
            let read = format!("sensed.Touch.{field}");
            let value = if field.ends_with("_valid") { format!("if {read} then 1 else 0") } else { read };
            let source = format!("{touch} gene sample:A={value};");
            let program = Program::compile(&source).unwrap();
            assert!(!program.participates_in_contact_signaling());
            let mut w=Writer::new();
            program.write_checkpoint(&mut w);
            for version in [9,10,11] {
                let minimum = if field.starts_with("signal") { 11 } else if field.starts_with("message") { 10 } else { 7 };
                assert_eq!(Program::read_checkpoint_version(&mut Reader::new(&w.0), version).is_ok(),
                    version >= minimum, "{field} in version {version}");
            }
        }
    }

    #[test]
    fn contact_declarations_are_typed_bounded_and_restricted_to_physical_contact() {
        for line in [
            "publish ri001;", "publish rb001;", "publish rf000;", "publish rf1000;",
            "publish rf001; publish rf002;", "avoid_message ri001;", "avoid_message rf001 = 0;",
            "publish rf001 = energy;", "publish rf001 = 1+2;", "publish rf001 = 1e14;",
            "adhere_message rf001 when true;", "rf001 = true;", "clear rf001 when 1;",
            "ri001=1;ri001=2;", "clear rb001;clear rb001;",
        ] { assert!(Program::compile(&format!("cellular; {line}")).is_err(), "{line}"); }
        let receptor = "cellular; product R=receptor(); product A=anchor(); product F=filament(); assembly Port: sense(contact) { R:.01; A:.01; F:.01; }";
        let source = format!("{receptor} rb001=sensed.Port.message_valid;rf001=sensed.Port.message;");
        let p = Program::compile(&source).unwrap();
        assert!(Program::compile(&source.replace("sense(contact)", "sense(heat)")).is_err());
        let mut memory = Registers::default();
        let mut scratch = EvalScratch::default();
        for (available, valid, message_valid, message, expected) in [
            (true,true,true,0.0,Some(0.0)), (true,true,true,-4.5,Some(-4.5)),
            (true,true,false,3.0,None), (true,false,true,3.0,None), (false,true,true,3.0,None),
        ] {
            let inputs = Inputs { receptors:vec![ReceptorInput { available,valid,message_valid,message,
                ..ReceptorInput::default() }], ..Inputs::default() };
            p.evaluate_cell(&inputs, &mut memory, &mut scratch);
            assert_eq!(memory.value("rb001"),Some(&Value::Boolean(expected.is_some())));
            assert_eq!(memory.value("rf001"),Some(&Value::Scalar(expected.unwrap_or(0.0))));
        }
        assert!(!p.participates_in_contact_signaling(), "reading a contact never exposes any private register");
        assert!(Program::compile("cellular; let publish=1; gene g: A=publish; product A=anchor();").is_ok(),
            "existing names are preserved because the new declaration keywords are contextual");
    }

    #[test]
    fn constructor_and_recipe_checks_reject_untyped_or_free_machinery() {
        for source in [
            "cellular; product A = \"motor\";",
            "cellular; product A = motor(2);",
            "cellular; product A = regulator();",
            "cellular; product A = regulator(0);",
            "cellular; product A = regulator(-1);",
            "cellular; product A = motor(); gene make: A = true;",
            "cellular; product A = motor(); gene make: Missing = 1;",
            "cellular; product A = motor(); gene make: A = pool.Missing;",
            "cellular; product A = motor(); gene make: A = built.Missing;",
            "cellular; product A = motor(); assembly Swim: flagellum { A: 0.04; }",
            "cellular; product A = motor(); assembly Swim: flagellum { A: 0; }",
            "cellular; product A = motor(); assembly Swim: flagellum { A: energy; }",
            "cellular; product A = motor(); assembly Swim: flagellum { A: 0.02; A: 0.02; }",
            "cellular; product A = scaffold(); assembly Shell: cortex { A: 0.04; activity: true; }",
            "cellular; product A = regulator(1); assembly Shell: cortex { A: 0.04; }",
            "cellular; product A = motor(); product A = motor();",
            "cellular; product A = motor(); gene make: A = 1; gene make: A = 0;",
        ] {
            assert!(Program::compile(source).is_err(), "accepted {source}");
        }
        let too_little_anchor = SWIMMER.replace("Anchor: 0.005", "Anchor: 0.00001");
        let error = Program::compile(&too_little_anchor).unwrap_err();
        assert!(error.contains("10% anchor"), "{error}");
        let unknown = "# 🌱\r\ncellular; product A=motor(); gene test: A=pool.B;";
        let diagnostic = Program::analyze(unknown).diagnostics.remove(0);
        let from = unknown[..unknown.find("pool.B").unwrap()]
            .encode_utf16()
            .count();
        assert_eq!((diagnostic.from, diagnostic.to), (from, from + 6));
    }

    #[test]
    fn inert_programs_and_passive_assemblies_have_honest_empty_roots() {
        let empty = Program::compile("cellular;").unwrap();
        assert!(empty.nodes.is_empty());
        assert_eq!(
            empty.evaluate(&Inputs::default()).cellular,
            Some(CellularControls::default())
        );
        let passive =
            "cellular; product Shell = scaffold(); assembly Wall: cortex { Shell: 0.04; }";
        let program = Program::compile(passive).unwrap();
        assert!(program.nodes.is_empty());
        assert_eq!(
            program
                .evaluate(&Inputs::default())
                .cellular
                .unwrap()
                .activity,
            [1.0]
        );
        let preview =
            Program::evaluation_json(passive, &Inputs::default(), &Registers::default(), 1, 1, 0)
                .unwrap();
        assert!(preview.contains(
            "\"name\":\"activity.Wall\",\"type\":\"scalar\",\"value\":1,\"from\":null,\"to\":null"
        ));
        assert!(program
            .graph_json()
            .contains("\"name\":\"activity.Wall\",\"type\":\"scalar\",\"value\":null"));
    }

    #[test]
    fn preview_and_graph_expose_actual_causal_gene_roots_and_named_stocks() {
        let program = Program::compile(SWIMMER).unwrap();
        let preview =
            Program::evaluation_json(SWIMMER, &inputs(), &Registers::default(), 7, 4, 12).unwrap();
        assert!(preview.contains("\"mode\":\"cellular\""));
        for item in [
            "\"name\":\"pool.Motor\"",
            "\"name\":\"built.Swim\"",
            "\"name\":\"gene.drive\"",
            "\"name\":\"activity.Swim\"",
        ] {
            assert!(preview.contains(item), "{item}");
        }
        assert!(!preview.contains("\"name\":\"target.flagellum.count\""));
        assert!(!preview.contains("\"name\":\"protein.flagellum\""));
        let graph = program.graph_json();
        assert!(graph.contains("\"label\":\"pool.Run\""));
        assert!(graph.contains("\"name\":\"gene.drive\""));
        assert!(!graph.contains("\"name\":\"thrust\""));
        let schema = schema_json();
        assert!(schema.contains("\"marker\":\"cellular;\""));
        assert!(schema.contains(
            "\"name\":\"flagellum\",\"requiredRoles\":[\"anchor\",\"motor\",\"filament\"]"
        ));
    }

    #[test]
    fn gene_mutations_preserve_declarations_and_comments_and_change_dosage() {
        let source = "# 🌱\r\ncellular;\r\nproduct Pulse = regulator(8);\r\ngene keep: Pulse = (0.5 # retain this\r\n); # tail\r\n";
        let silence = Program::mutate_source(source, 1, MutationKind::GeneSilence).unwrap();
        assert_eq!(silence.kind, MutationKind::GeneSilence);
        assert_eq!(
            silence
                .program
                .evaluate(&Inputs::default())
                .cellular
                .unwrap()
                .expression,
            [0.0]
        );
        assert!(silence.source.contains("0 * ((0.5 # retain this\r\n))"));
        assert!(silence.source.ends_with("; # tail\r\n"));
        let duplicate = Program::mutate_source(source, 1, MutationKind::GeneDuplicate).unwrap();
        assert_eq!(
            duplicate
                .program
                .evaluate(&Inputs::default())
                .cellular
                .unwrap()
                .expression,
            [1.0]
        );
        assert!(duplicate
            .source
            .contains("gene keep_2: Pulse = (0.5 # retain this\r\n);"));
        assert!(duplicate
            .source
            .starts_with(&source[..source.find("; # tail").unwrap() + 1]));
        assert_eq!(
            duplicate.program.cellular(),
            Program::compile(source).unwrap().cellular()
        );
        let next =
            Program::mutate_source(&duplicate.source, 2, MutationKind::GeneDuplicate).unwrap();
        assert_eq!(next.program.cellular.as_ref().unwrap().genes.len(), 3);
    }

    #[test]
    fn regulatory_mutation_chains_keep_physical_definitions_and_never_add_registers() {
        let original = Program::compile(SWIMMER).unwrap();
        let mut source = SWIMMER.to_string();
        for seed in 1..=120 {
            let kinds = [
                MutationKind::Constant,
                MutationKind::Expression,
                MutationKind::Reference,
                MutationKind::GeneSilence,
                MutationKind::GeneDuplicate,
            ];
            let kind = kinds[seed as usize % kinds.len()];
            let Some(mutation) = Program::mutate_source(&source, seed, kind) else {
                continue;
            };
            assert_eq!(mutation.program.cellular(), original.cellular());
            assert!(!mutation
                .program
                .nodes
                .iter()
                .any(|node| matches!(node.op, Op::Register(_) | Op::Populated(_))));
            assert!(mutation.program.writes.is_empty());
            let again = Program::mutate_source(&source, seed, kind).unwrap();
            assert_eq!(mutation.source, again.source);
            let output = mutation.program.evaluate(&inputs()).cellular.unwrap();
            assert!(output.expression.iter().all(|v| v.is_finite() && *v >= 0.0));
            assert!(output.activity.iter().all(|v| (0.0..=1.0).contains(v)));
            source = mutation.source;
        }
    }

    #[test]
    fn cellular_checkpoints_round_trip_and_reject_corrupted_types_and_recipes() {
        for source in [SWIMMER, "cellular;"] {
            let program = Program::compile(source).unwrap();
            let mut writer = Writer::new();
            program.write_checkpoint(&mut writer);
            assert_eq!(&writer.0[..4], &u32::MAX.to_le_bytes());
            assert!(Program::read_checkpoint_version(&mut Reader::new(&writer.0), 4).is_err());
            let restored = Program::read_checkpoint(&mut Reader::new(&writer.0)).unwrap();
            assert_eq!(program.evaluate(&inputs()), restored.evaluate(&inputs()));
            assert_eq!(program.cellular(), restored.cellular());
            assert!(restored.checkpoint_source_matches(&program, source, true));
        }
        for corruption in 0..5 {
            let mut program = Program::compile(SWIMMER).unwrap();
            match corruption {
                0 => program.cellular.as_mut().unwrap().genes[0].product = 200,
                1 => program.cellular.as_mut().unwrap().definition.assemblies[0].parts[0].1 = 0.0,
                2 => program.nodes[0].op = Op::Pool(300),
                3 => program.cellular.as_mut().unwrap().definition.products[3].half_life = 0.0,
                _ => program.cellular.as_mut().unwrap().genes[0].name = "time".into(),
            }
            let mut writer = Writer::new();
            program.write_checkpoint(&mut writer);
            assert!(
                Program::read_checkpoint(&mut Reader::new(&writer.0)).is_err(),
                "corruption {corruption}"
            );
        }
    }

    fn all_mechanisms_source() -> String {
        let mut source = String::from("cellular;\n");
        for (index, kind) in ProductKind::ALL.iter().enumerate() {
            source.push_str(&format!(
                "product Part{index} = {}({});\n",
                kind.name(),
                if *kind == ProductKind::Regulator {
                    "8"
                } else {
                    ""
                },
            ));
        }
        for (index, kind) in MechanismKind::ALL.iter().enumerate() {
            source.push_str(&format!("assembly Machine{index}: {} {{\n", kind.name()));
            for role in kind.required_roles() {
                source.push_str(&format!("  Part{}: 0.04;\n", *role as usize));
            }
            source.push_str("  activity: if energy > 30 then 0.75 else 0;\n}\n");
        }
        source
    }

    #[test]
    fn mechanisms_use_typed_contextual_constructors_with_forward_reads() {
        let source = r#"cellular;
product support = membrane();
product Catalyst = enzyme();
gene fill: support = if built.Tank < 0.08 && pool.support < 0.002 then 1 else 0;
assembly Tank: store ( # a typed target, not a dynamic expression
  food
) { support: 0.08; }
assembly Digest: digest(tissue) { Catalyst: 0.04; activity: if gut.tissue.energy > 0 then 1 else 0; }
"#;
        let program = Program::compile(source).unwrap();
        let definition = program.cellular().unwrap();
        assert_eq!(definition.assemblies[0].kind.name(), "store(food)");
        assert_eq!(definition.assemblies[1].kind.name(), "digest(tissue)");
        let output = program
            .evaluate(&Inputs {
                products: vec![0.0, 0.0],
                assemblies: vec![0.0, 0.04],
                ..Inputs::default()
            })
            .cellular
            .unwrap();
        assert_eq!(output.expression, [1.0, 0.0]);
        assert_eq!(output.activity, [1.0, 0.0]);
        let idle = program
            .evaluate(&Inputs {
                products: vec![0.002, 0.0],
                assemblies: vec![0.08, 0.04],
                ..Inputs::default()
            })
            .cellular
            .unwrap();
        assert_eq!(idle.expression, [0.0, 0.0]);
    }

    #[test]
    fn mechanisms_reject_wrong_targets_arity_and_unpaid_roles() {
        for constructor in [
            "store()",
            "store(tissue)",
            "store(\"food\")",
            "store(energy, mineral)",
            "store(1)",
            "store(food + 1)",
            "store(if energy > 0 then food else mineral)",
            "digest(food)",
            "digest(light)",
            "sense(food)",
            "harvest(tissue)",
            "import(energy)",
            "translate(food)",
            "propel(0)",
            "unknown(food)",
            "translate",
            "digest",
            "photosynthetic()",
        ] {
            let source = format!(
                "cellular; product Wall=membrane(); assembly Test: {constructor} {{ Wall: 0.04; }}"
            );
            assert!(Program::compile(&source).is_err(), "accepted {constructor}");
        }
        for constructor in [
            "digest(tissue)",
            "harvest(light)",
            "propel()",
            "translate()",
        ] {
            let source = format!(
                "cellular; product Frame=scaffold(); assembly Test: {constructor} {{ Frame: 0.04; }}"
            );
            assert!(
                Program::compile(&source).is_err(),
                "unpaid roles in {constructor}"
            );
        }
        let source = "cellular; product E=enzyme(); assembly A: repair { E: 0.04; } assembly B: repair() { E: 0.04; }";
        let program = Program::compile(source).unwrap();
        assert_ne!(
            program.cellular().unwrap().assemblies[0].kind,
            program.cellular().unwrap().assemblies[1].kind
        );
        assert!(matches!(
            program.cellular().unwrap().assemblies[0].kind,
            AssemblyKind::Organelle(_)
        ));
        assert!(matches!(
            program.cellular().unwrap().assemblies[1].kind,
            AssemblyKind::Mechanism(_)
        ));
    }

    #[test]
    fn every_mechanism_is_exposed_and_checkpointed_without_retyping_old_families() {
        let source = all_mechanisms_source();
        let program = Program::compile(&source).unwrap();
        let schema = schema_json();
        for (kind, assembly) in MechanismKind::ALL
            .iter()
            .zip(&program.cellular().unwrap().assemblies)
        {
            assert_eq!(assembly.kind, AssemblyKind::Mechanism(*kind));
            assert!(schema.contains(&format!("\"name\":{}", quote(kind.name()))));
            assert!(schema.contains(&format!("\"description\":{}", quote(kind.description()))));
        }
        let mut writer = Writer::new();
        program.write_checkpoint(&mut writer);
        assert!(Program::read_checkpoint_version(&mut Reader::new(&writer.0), 5).is_err());
        assert!(Program::read_checkpoint_version(&mut Reader::new(&writer.0), 6).is_err());
        assert!(Program::read_checkpoint_version(&mut Reader::new(&writer.0), 7).is_err());
        assert!(Program::read_checkpoint_version(&mut Reader::new(&writer.0), 8).is_err());
        assert!(Program::read_checkpoint_version(&mut Reader::new(&writer.0), 9).is_err());
        assert!(Program::read_checkpoint_version(&mut Reader::new(&writer.0), 10).is_err());
        let restored = Program::read_checkpoint_version(&mut Reader::new(&writer.0), 11).unwrap();
        assert_eq!(program.cellular(), restored.cellular());
        assert_eq!(program.evaluate(&inputs()), restored.evaluate(&inputs()));
        assert!(restored.checkpoint_source_matches(&program, &source, true));

        let legacy = Program::compile(SWIMMER).unwrap();
        let mut writer = Writer::new();
        legacy.write_checkpoint(&mut writer);
        let restored = Program::read_checkpoint_version(&mut Reader::new(&writer.0), 5).unwrap();
        assert_eq!(legacy.cellular(), restored.cellular());
        assert_eq!(legacy.evaluate(&inputs()), restored.evaluate(&inputs()));
    }

    #[test]
    fn regulatory_mutations_keep_targets_and_physical_recipes_exact() {
        let source = all_mechanisms_source();
        let original = Program::compile(&source).unwrap();
        let mut mutated = source;
        for seed in 1..=40 {
            let kinds = [
                MutationKind::Constant,
                MutationKind::Expression,
                MutationKind::Reference,
                MutationKind::GeneSilence,
                MutationKind::GeneDuplicate,
            ];
            let Some(result) =
                Program::mutate_source(&mutated, seed, kinds[seed as usize % kinds.len()])
            else {
                continue;
            };
            assert_eq!(result.program.cellular(), original.cellular());
            let mut writer = Writer::new();
            result.program.write_checkpoint(&mut writer);
            let restored = Program::read_checkpoint(&mut Reader::new(&writer.0)).unwrap();
            assert_eq!(restored.cellular(), original.cellular());
            assert_eq!(
                restored.evaluate(&inputs()),
                result.program.evaluate(&inputs())
            );
            mutated = result.source;
        }
    }
}
