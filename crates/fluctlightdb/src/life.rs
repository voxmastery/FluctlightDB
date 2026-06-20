use uuid::Uuid;

use serde::{Deserialize, Serialize};

/// One agent life — Return by Death namespace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LifeState {
    pub life_id: Uuid,
    pub started_at_tick: u64,
    pub death_count: u32,
    pub alive: bool,
}

impl LifeState {
    pub fn birth(tick: u64) -> Self {
        Self {
            life_id: Uuid::new_v4(),
            started_at_tick: tick,
            death_count: 0,
            alive: true,
        }
    }

    pub fn death(&mut self) {
        self.alive = false;
        self.death_count += 1;
    }

    pub fn respawn(&mut self, tick: u64) -> Uuid {
        self.life_id = Uuid::new_v4();
        self.started_at_tick = tick;
        self.alive = true;
        self.life_id
    }
}

/// Survives life reset — identity, values, hard-won lessons.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoreMemory {
    pub key: String,
    pub content: String,
    pub from_life: Uuid,
    pub engram_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoreMemoryStore {
    pub memories: Vec<CoreMemory>,
}

impl CoreMemoryStore {
    pub fn persist(&mut self, key: String, content: String, life: Uuid, engram_id: Option<Uuid>) {
        if let Some(m) = self.memories.iter_mut().find(|m| m.key == key) {
            m.content = content;
            m.from_life = life;
            m.engram_id = engram_id;
        } else {
            self.memories.push(CoreMemory {
                key,
                content,
                from_life: life,
                engram_id,
            });
        }
    }
}
