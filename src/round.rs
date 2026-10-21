#[derive(Clone, Debug)]
pub struct Clip {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl Clip {
    pub fn duration(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate as f32
    }
}

pub fn reverse(clip: &Clip) -> Clip {
    Clip {
        samples: clip.samples.iter().rev().copied().collect(),
        sample_rate: clip.sample_rate,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    RecordOriginal,
    ReviewOriginal,
    PlayReversed,
    RecordAttempt,
    Results,
}

pub struct Round {
    pub step: Step,
    pub original: Option<Clip>,
    pub reversed: Option<Clip>,
    pub attempt: Option<Clip>,
    pub rereversed: Option<Clip>,
}

impl Default for Round {
    fn default() -> Self {
        Self::new()
    }
}

impl Round {
    pub fn new() -> Self {
        Self {
            step: Step::RecordOriginal,
            original: None,
            reversed: None,
            attempt: None,
            rereversed: None,
        }
    }

    pub fn finish_original(&mut self, clip: Clip) {
        self.original = Some(clip);
        self.step = Step::ReviewOriginal;
    }

    pub fn rerecord(&mut self) {
        self.original = None;
        self.step = Step::RecordOriginal;
    }

    pub fn confirm_original(&mut self) {
        let Some(original) = &self.original else {
            return;
        };

        self.reversed = Some(reverse(original));
        self.step = Step::PlayReversed;
    }

    pub fn begin_attempt(&mut self) {
        self.step = Step::RecordAttempt;
    }

    pub fn finish_attempt(&mut self, clip: Clip) {
        self.rereversed = Some(reverse(&clip));
        self.attempt = Some(clip);
        self.step = Step::Results;
    }
}
