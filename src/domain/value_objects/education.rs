use bevy::prelude::Resource;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EducationMode {
    Simulation,
    Education,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JournalCategory {
    VacuumSuperfluid,
    AsymmetricPolarization,
    ZpeExtraction,
    MetricEngineering,
    QuranicEvidence,
}

impl JournalCategory {
    pub fn label(&self) -> &'static str {
        match self {
            JournalCategory::VacuumSuperfluid => "Vacuum Superfluid",
            JournalCategory::AsymmetricPolarization => "Asymmetric Vacuum Polarization",
            JournalCategory::ZpeExtraction => "ZPE Extraction",
            JournalCategory::MetricEngineering => "Metric Engineering",
            JournalCategory::QuranicEvidence => "Quranic Evidence",
        }
    }
}

#[derive(Debug, Clone)]
pub struct QuranicReference {
    pub sura: u32,
    pub verse: u32,
    pub arabic: &'static str,
    pub translation: &'static str,
    pub explanation: &'static str,
}

#[derive(Debug, Clone)]
pub struct JournalEntry {
    pub id: &'static str,
    pub title: &'static str,
    pub category: JournalCategory,
    pub body: &'static [&'static str],
    pub quranic_refs: Vec<QuranicReference>,
    pub formula: Option<&'static str>,
    pub unlock: UnlockCondition,
}

#[derive(Debug, Clone)]
pub enum UnlockCondition {
    Immediate,
    CraftSpawned,
    PulseAbove(f32),
    AltitudeAbove(f32),
    SpeedAbove(f32),
    OrbitAchieved,
    Landed,
    TimeElapsed(f32),
}

#[derive(Resource)]
pub struct JournalDatabase {
    pub entries: Vec<JournalEntry>,
    pub unlocked: Vec<bool>,
    pub just_unlocked: Vec<usize>,
}

impl JournalDatabase {
    pub fn new(entries: Vec<JournalEntry>) -> Self {
        let count = entries.len();
        Self {
            entries,
            unlocked: vec![false; count],
            just_unlocked: Vec::new(),
        }
    }

    pub fn unlock(&mut self, index: usize) {
        if index < self.unlocked.len() && !self.unlocked[index] {
            self.unlocked[index] = true;
            self.just_unlocked.push(index);
        }
    }

    pub fn is_unlocked(&self, index: usize) -> bool {
        self.unlocked.get(index).copied().unwrap_or(false)
    }

    pub fn drain_notifications(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.just_unlocked)
    }
}

#[derive(Resource)]
pub struct EducationState {
    pub panel_open: bool,
    pub mode: EducationMode,
    pub current_category: JournalCategory,
    pub current_entry_index: Option<usize>,
    pub journal_section_open: bool,
    pub show_field_gradient: bool,
    pub show_particles: bool,
    pub show_ripples: bool,
    pub flight_time: f32,
}

impl Default for EducationState {
    fn default() -> Self {
        Self {
            panel_open: false,
            mode: EducationMode::Education,
            current_category: JournalCategory::VacuumSuperfluid,
            current_entry_index: None,
            journal_section_open: false,
            show_field_gradient: true,
            show_particles: true,
            show_ripples: true,
            flight_time: 0.0,
        }
    }
}
