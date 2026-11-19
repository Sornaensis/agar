use super::*;

#[derive(Clone, Copy, Default)]
pub(super) struct CellSignal {
    pub(super) published: Option<f64>,
    pub(super) signal: Option<f64>,
    pub(super) avoid: Option<f64>,
    pub(super) adhesion: Option<f64>,
}

/// Both sides must supply occupied values. A stored zero is a valid signal.
pub(super) fn avoids_contact(signals: &[CellSignal], actor: usize, candidate: usize) -> bool {
    signals.get(actor).and_then(|signal| signal.avoid).is_some_and(|local| {
        signals[candidate].published == Some(local)
    })
}

impl World {
    /// Physical interactions occur after program writes, so keep only the
    /// explicitly declared values from the same snapshot as contact sensing.
    pub(super) fn freeze_contact_signals(&self, genomes: &[Genome], target: &mut Vec<CellSignal>) {
        target.clear();
        if !genomes.iter().any(|genome| genome.program.participates_in_contact_signaling()) {
            return;
        }
        target.extend(self.cells.iter().map(|cell| {
            let program = &genomes[(cell.genome_id - 1) as usize].program;
            CellSignal {
                published: program.published_message(&cell.memory),
                signal: program.contact_signal(&cell.memory),
                avoid: program.avoid_message(&cell.memory),
                adhesion: program.adhesion_message(&cell.memory),
            }
        }));
    }
}
