//! A contact guard runs for one physical interface. It shares the genome's pure
//! operations and reads the acting cell's pre-write register snapshot.
use super::*;
use crate::body::OrganelleKind;
use crate::cellular::MechanismKind;

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct PeerContact {
    pub message: Option<f64>,
    pub signal: Option<f64>,
    pub division_interface: bool,
    pub resistance: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub(super) enum PeerField {
    MessageValid,
    Message,
    SignalValid,
    Signal,
    DivisionInterface,
    Resistance,
    ResistanceValid,
}

impl PeerField {
    pub(super) const ALL: [Self; 7] = [
        Self::MessageValid,
        Self::Message,
        Self::SignalValid,
        Self::Signal,
        Self::DivisionInterface,
        Self::Resistance,
        Self::ResistanceValid,
    ];

    pub(super) fn lookup(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.name() == name)
    }
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::MessageValid => "peer.message_valid",
            Self::Message => "peer.message",
            Self::SignalValid => "peer.signal_valid",
            Self::Signal => "peer.signal",
            Self::DivisionInterface => "peer.division_interface",
            Self::Resistance => "peer.resistance",
            Self::ResistanceValid => "peer.resistance_valid",
        }
    }
    pub(super) fn ty(self) -> Type {
        match self {
            Self::Message | Self::Signal | Self::Resistance => Type::Number,
            _ => Type::Bool,
        }
    }
    pub(super) fn read(self, peer: PeerContact) -> Value {
        match self {
            Self::MessageValid => Value::Boolean(peer.message.is_some()),
            Self::Message => Value::Scalar(peer.message.unwrap_or(0.0)),
            Self::SignalValid => Value::Boolean(peer.signal.is_some()),
            Self::Signal => Value::Scalar(peer.signal.unwrap_or(0.0)),
            Self::DivisionInterface => Value::Boolean(peer.division_interface),
            Self::Resistance => Value::Scalar(peer.resistance.unwrap_or(0.0)),
            Self::ResistanceValid => Value::Boolean(peer.resistance.is_some()),
        }
    }
}

pub(super) fn supports_guard(kind: AssemblyKind) -> bool {
    matches!(
        kind,
        AssemblyKind::Mechanism(
            MechanismKind::Pierce | MechanismKind::ParticleCapture | MechanismKind::Engulf | MechanismKind::Adhere
        ) | AssemblyKind::Organelle(OrganelleKind::Piercer | OrganelleKind::Phagocyte)
    )
}

pub(super) fn default_guard(kind: AssemblyKind) -> bool {
    kind != AssemblyKind::Mechanism(MechanismKind::Adhere)
}

#[derive(Debug, Clone)]
pub(super) struct ContactGuards {
    /// Authored (recipe, expression root) pairs for graph inspection and saves.
    pub roots: Box<[(usize, usize)]>,
    has_adhesion: bool,
    registers: Box<[Register]>,
    plan: Box<ExecutionPlan>,
    /// Recipes may have structurally identical guards. Index each one into a
    /// shared list of the executable nodes its distinct root actually needs.
    schedules: Box<[Box<[usize]>]>,
    recipe_schedules: Box<[usize]>,
}
impl ContactGuards {
    pub fn compile(
        nodes: &[Node],
        roots: Vec<(usize, usize)>,
        definition: &CellularProgram,
    ) -> Option<Box<Self>> {
        if roots.is_empty() {
            return None;
        }
        let plan = ExecutionPlan::compile(
            nodes,
            &[None; OUTPUT_COUNT],
            &[],
            &roots.iter().map(|&(_, root)| root).collect::<Vec<_>>(),
        );
        let registers = plan
            .nodes
            .iter()
            .filter_map(|node| match node.op {
                Op::Register(key) | Op::Populated(key) => Some(key),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut schedules = Vec::new();
        let mut by_root = HashMap::new();
        let recipe_schedules = plan
            .extra_roots
            .iter()
            .map(|&root| {
                *by_root.entry(root).or_insert_with(|| {
                    let mut reached = vec![false; root + 1];
                    let mut pending = vec![root];
                    while let Some(index) = pending.pop() {
                        if reached[index] {
                            continue;
                        }
                        reached[index] = true;
                        let (children, count) = plan.nodes[index].op.children();
                        pending.extend_from_slice(&children[..count]);
                    }
                    let schedule = reached
                        .iter()
                        .enumerate()
                        .filter_map(|(index, &needed)| needed.then_some(index))
                        .collect();
                    let slot = schedules.len();
                    schedules.push(schedule);
                    slot
                })
            })
            .collect();
        let has_adhesion = roots.iter().any(|&(index, _)| {
            definition.assemblies[index].kind == AssemblyKind::Mechanism(MechanismKind::Adhere)
        });
        Some(Box::new(Self {
            roots: roots.into(),
            has_adhesion,
            registers,
            plan,
            schedules: schedules.into(),
            recipe_schedules,
        }))
    }
    pub fn allocated_bytes(&self) -> usize {
        std::mem::size_of_val(&*self.roots) + self.derived_bytes()
    }
    pub fn derived_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + std::mem::size_of_val(&*self.registers)
            + self.plan.allocated_bytes
            + std::mem::size_of_val(&*self.recipe_schedules)
            + std::mem::size_of_val(&*self.schedules)
            + self
                .schedules
                .iter()
                .map(|schedule| std::mem::size_of_val(&**schedule))
                .sum::<usize>()
    }
    fn slot(&self, recipe: usize) -> Option<usize> {
        self.roots
            .binary_search_by_key(&recipe, |entry| entry.0)
            .ok()
    }
    fn evaluate(
        &self,
        slot: usize,
        frozen: &[Option<Value>],
        peer: PeerContact,
        values: &mut Vec<Value>,
    ) -> bool {
        let root = self.plan.extra_roots[slot];
        if values.len() <= root {
            values.resize_with(root + 1, || Value::Boolean(false));
        }
        for &index in &*self.schedules[self.recipe_schedules[slot]] {
            let node = &self.plan.nodes[index];
            let value = match node.op {
                Op::Peer(field) => field.read(peer),
                Op::Register(key) | Op::Populated(key) => {
                    let index = self
                        .registers
                        .binary_search(&key)
                        .expect("compiled contact register");
                    let stored = frozen.get(index).and_then(Option::as_ref);
                    if matches!(node.op, Op::Populated(_)) {
                        Value::Boolean(stored.is_some())
                    } else {
                        stored.cloned().unwrap_or_else(|| Value::zero(key.ty))
                    }
                }
                _ => evaluate_pure(&node.op, node.ty, values),
            };
            values[index] = value;
        }
        values[root].boolean()
    }
}

pub(super) fn input_dependencies(nodes: &[Node]) -> (Vec<bool>, Vec<bool>) {
    let mut peer = vec![false; nodes.len()];
    let mut samples_local = vec![false; nodes.len()];
    for (i, node) in nodes.iter().enumerate() {
        let (children, len) = node.op.children();
        let inputs = &children[..len];
        peer[i] = matches!(node.op, Op::Peer(_)) || inputs.iter().any(|&j| peer[j]);
        samples_local[i] = matches!(
            node.op,
            Op::Sensor(_) | Op::Receptor(..) | Op::Pool(_) | Op::Concentration(_) | Op::Built(_)
        ) || inputs.iter().any(|&j| samples_local[j]);
    }
    (peer, samples_local)
}

/// Validate the two execution contexts after resolving aliases and function
/// parameters. A peer-dependent binding may be shared by several guards.
pub(super) fn validate_contexts(
    nodes: &[Node],
    guards: &[(usize, usize)],
    local_roots: impl Iterator<Item = usize>,
) -> Result<(), (usize, &'static str)> {
    if guards.is_empty() && !nodes.iter().any(|n| matches!(n.op, Op::Peer(_))) {
        return Ok(());
    }
    let (peer, samples_local) = input_dependencies(nodes);
    for &(_, root) in guards {
        if samples_local[root] {
            return Err((root, "engage_when reads peer fields and local registers. Store a cell measurement in a register to use it here."));
        }
    }
    for root in local_roots {
        if peer[root] {
            return Err((
                root,
                "peer fields require an engage_when expression on contact hardware.",
            ));
        }
    }
    Ok(())
}

impl Program {
    /// Keep reference mutations inside the evaluation contexts that use this
    /// node. Bindings shared with cell-wide outputs may only acquire inputs
    /// common to both contexts.
    pub(super) fn contact_reference_options(&self, index: usize, options: &mut Vec<Op>) {
        let Some(graph) = self.cellular.as_ref() else {
            return;
        };
        if graph.contact_roots().is_empty() {
            return;
        }
        let mut uses = vec![0_u8; self.nodes.len()];
        for root in graph.roots().into_iter().chain(
            self.writes
                .iter()
                .flat_map(|w| w.value.into_iter().chain(w.guard)),
        ) {
            uses[root] |= 1;
        }
        for &(_, root) in graph.contact_roots() {
            uses[root] |= 2;
        }
        for i in (index + 1..self.nodes.len()).rev() {
            let (children, len) = self.nodes[i].op.children();
            let flags = uses[i];
            for &child in &children[..len] {
                uses[child] |= flags;
            }
        }
        let (peer, local) = input_dependencies(&self.nodes);
        let guard = uses[index] & 2 != 0;
        let ordinary = uses[index] & 1 != 0;
        options.retain(|op| match op {
            Op::Sensor(_)
            | Op::Pool(_)
            | Op::Built(_)
            | Op::Concentration(_)
            | Op::Receptor(..) => !guard,
            Op::Reference(i) => !(guard && local[*i] || ordinary && peer[*i]),
            _ => true,
        });
        if guard && !ordinary {
            let node = &self.nodes[index];
            let current = &self.nodes[self.reference_root(index)].op;
            options.extend(
                PeerField::ALL
                    .into_iter()
                    .filter(|field| field.ty() == node.ty)
                    .map(Op::Peer)
                    .filter(|op| op != current),
            );
        }
    }
    pub(crate) fn has_adhesion_guard(&self) -> bool {
        self.cellular
            .as_ref()
            .and_then(|graph| graph.engagement.as_ref())
            .is_some_and(|guards| guards.has_adhesion)
    }
    pub(crate) fn has_contact_guards(&self) -> bool {
        self.cellular
            .as_ref()
            .is_some_and(|graph| graph.engagement.is_some())
    }
    pub(crate) fn has_contact_guard(&self, assembly: usize) -> bool {
        self.cellular
            .as_ref()
            .and_then(|graph| graph.engagement.as_deref())
            .is_some_and(|guards| guards.slot(assembly).is_some())
    }
    pub(crate) fn freeze_contact_registers(
        &self,
        memory: &Registers,
        frozen: &mut Vec<Option<Value>>,
    ) {
        frozen.clear();
        if let Some(guards) = self
            .cellular
            .as_ref()
            .and_then(|graph| graph.engagement.as_ref())
        {
            frozen.extend(
                guards
                    .registers
                    .iter()
                    .map(|key| memory.entries.get(key).cloned()),
            );
        }
    }
    pub(crate) fn allows_contact(
        &self,
        assembly: usize,
        frozen: &[Option<Value>],
        peer: PeerContact,
        scratch: &mut Vec<Value>,
    ) -> bool {
        let Some(graph) = self.cellular.as_deref() else {
            return true;
        };
        if let Some(guards) = graph.engagement.as_ref() {
            if let Some(slot) = guards.slot(assembly) {
                return guards.evaluate(slot, frozen, peer, scratch);
            }
        }
        graph
            .definition
            .assemblies
            .get(assembly)
            .is_none_or(|recipe| default_guard(recipe.kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::{Reader, Writer};

    const PREFACE: &str = "cellular; product A=anchor(); product F=filament(); product E=enzyme();
        publish rf001=0; let match_marker=peer.message_valid && populated(rf001) && peer.message==rf001;";
    const WEAPON: &str = "assembly Tip:pierce(){A:0.01;F:0.03;E:0.01;engage_when:!match_marker;}";
    const SECOND: &str = "assembly Probe:pierce(){A:0.01;F:0.03;E:0.01;engage_when:peer.signal_valid && peer.signal<0;}";
    #[test]
    fn each_contact_recipe_reads_actual_peer_and_frozen_private_memory() {
        let source = format!("{PREFACE}{WEAPON}{SECOND}");
        let p = Program::compile(&source).unwrap();
        let mut memory = Registers::default();
        let mut frozen = Vec::new();
        let mut tmp = Vec::new();
        let peer = PeerContact {
            resistance: None,
            message: Some(0.0),
            signal: Some(-2.0),
            division_interface: false,
        };
        p.freeze_contact_registers(&memory, &mut frozen);
        p.initialize_message(&mut memory);
        assert!(
            p.allows_contact(0, &frozen, peer, &mut tmp),
            "register was empty at snapshot"
        );
        p.freeze_contact_registers(&memory, &mut frozen);
        assert!(!p.allows_contact(0, &frozen, peer, &mut tmp));
        assert!(p.allows_contact(1, &frozen, peer, &mut tmp));
        assert!(p.allows_contact(0, &frozen, PeerContact::default(), &mut tmp));
        assert!(!p.allows_contact(1, &frozen, PeerContact::default(), &mut tmp));
        let mut w = Writer::new();
        p.write_checkpoint(&mut w);
        assert!(Program::read_checkpoint_version(&mut Reader::new(&w.0), 18).is_err());
        let restored = Program::read_checkpoint_version(&mut Reader::new(&w.0), 19).unwrap();
        assert!(restored.checkpoint_source_matches(&p, &source, true));
        assert!(!restored.allows_contact(0, &frozen, peer, &mut tmp));
        let graph = p.graph_json();
        assert!(graph.contains("output:engage_when.Tip") && graph.contains("peer.message_valid"));
        let preview =
            Program::evaluation_json(&source, &Inputs::default(), &memory, 1, 1, 0).unwrap();
        assert!(preview.contains(
            r#""name":"engage_when.Tip","type":"boolean","value":null,"perContact":true"#
        ));
    }

    #[test]
    fn independent_contact_outputs_only_run_their_own_shared_dependencies() {
        let third = WEAPON.replace("assembly Tip", "assembly Spare");
        let p = Program::compile(&format!("{PREFACE}{WEAPON}{SECOND}{third}")).unwrap();
        let guards = p.cellular.as_ref().unwrap().engagement.as_ref().unwrap();
        assert_eq!(guards.schedules.len(), 2);
        assert_eq!(guards.recipe_schedules[0], guards.recipe_schedules[2]);
        let first = &guards.schedules[guards.recipe_schedules[0]];
        let second = &guards.schedules[guards.recipe_schedules[1]];
        assert!(first.len() < guards.plan.nodes.len() && second.len() < guards.plan.nodes.len());
        assert!(first.iter().all(|&i| !matches!(
            guards.plan.nodes[i].op,
            Op::Peer(PeerField::Signal | PeerField::SignalValid)
        )));
        assert!(second
            .iter()
            .all(|&i| !matches!(guards.plan.nodes[i].op, Op::Register(_) | Op::Populated(_))));
        let a = Program::analyze(&format!("{PREFACE} let warm=energy>0;{WEAPON}"));
        assert!(a.valid);
        let marker = a
            .bindings
            .iter()
            .find(|b| b.name == "match_marker")
            .unwrap();
        assert!(marker.per_contact && marker.contact_safe);
        let warm = a.bindings.iter().find(|b| b.name == "warm").unwrap();
        assert!(!warm.per_contact && !warm.contact_safe);
    }

    #[test]
    fn a_historical_product_can_keep_its_name_and_a_recipe_can_add_a_guard() {
        let source =
            "cellular; product engage_when=anchor(); product F=filament(); product E=enzyme();
            assembly Tip:pierce(){engage_when:0.01; F:0.03; E:0.01;}";
        let old = Program::compile_historical_contact_source(source).unwrap();
        let current = Program::compile(source).unwrap();
        assert!(old.checkpoint_source_matches(&current, source, true));
        let guarded =
            Program::compile(&source.replace("E:0.01;", "E:0.01; engage_when:peer.signal_valid;"))
                .unwrap();
        assert!(guarded.has_contact_guard(0));
        assert_eq!(guarded.cellular().unwrap().assemblies[0].parts.len(), 3);
    }

    #[test]
    fn guard_context_is_checked_transitively_and_old_declarations_are_rejected() {
        for field in [
            "avoid_message rf001;",
            "adhere_message rf001 recruit;",
            "rb001=match_marker;",
            "product S=scaffold();gene g:S=if match_marker then 0 else 1;",
        ] {
            assert!(
                Program::compile(&format!("{PREFACE}{field}")).is_err(),
                "{field}"
            );
        }
        for (src, problem) in [
            (
                format!(
                    "{PREFACE}let energetic=energy>0;{}",
                    WEAPON.replace("!match_marker", "match_marker && energetic")
                ),
                "measure",
            ),
            (
                format!(
                    "{PREFACE}{}",
                    WEAPON.replace("!match_marker", "peer.signal")
                ),
                "boolean",
            ),
            (
                format!(
                    "{PREFACE}{}",
                    WEAPON.replace(
                        "engage_when:",
                        "activity:if match_marker then 1 else 0; engage_when:"
                    )
                ),
                "engage_when",
            ),
            (
                "cellular; product E=enzyme(); assembly R:repair(){E:0.02;engage_when:true;}"
                    .into(),
                "surface",
            ),
        ] {
            let error = Program::compile(&src).unwrap_err();
            assert!(error.contains(problem), "{problem} – {error}");
        }
        assert!(Program::compile("cellular; let surfaceOnly=peer.signal_valid;").is_ok());
        assert!(Program::compile_stored("cellular; avoid_message rf001;").is_err());
    }
}
