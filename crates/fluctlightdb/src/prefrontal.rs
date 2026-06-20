use serde::{Deserialize, Serialize};

/// Executive region — goals, inhibition (unlocks at Adolescent stage).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Prefrontal {
    pub goals: Vec<String>,
    pub inhibit_actions: Vec<String>,
    pub unlocked: bool,
}

impl Prefrontal {
    pub fn set_goal(&mut self, goal: String) {
        if self.unlocked {
            self.goals.push(goal);
        }
    }

    pub fn inhibit(&mut self, action: String) {
        if self.unlocked {
            self.inhibit_actions.push(action);
        }
    }

    pub fn is_inhibited(&self, action: &str) -> bool {
        self.unlocked && self.inhibit_actions.iter().any(|a| a == action)
    }
}
