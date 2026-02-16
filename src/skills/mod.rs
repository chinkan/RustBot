pub mod loader;

use std::collections::HashMap;
use tracing::info;

/// A loaded skill from a markdown file
#[derive(Debug, Clone)]
pub struct Skill {
    /// Skill name (derived from filename or frontmatter)
    pub name: String,
    /// Short description
    pub description: String,
    /// Full markdown content (the instructions)
    pub content: String,
    /// Category/tags for organization
    pub tags: Vec<String>,
}

/// Registry of all loaded skills
#[derive(Debug, Clone)]
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    /// Register a skill
    pub fn register(&mut self, skill: Skill) {
        info!("Registered skill: {} — {}", skill.name, skill.description);
        self.skills.insert(skill.name.clone(), skill);
    }

    /// Get a skill by name
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    /// List all registered skills
    pub fn list(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }

    /// Build context string for the system prompt.
    /// Gives the LLM awareness of all available skills.
    pub fn build_context(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }

        let mut context = String::from(
            "You have the following skills available. When relevant, follow these instructions:\n\n",
        );
        for skill in self.skills.values() {
            context.push_str(&format!("## Skill: {}\n", skill.name));
            context.push_str(&format!("{}\n\n", skill.content));
        }
        context
    }

    pub fn len(&self) -> usize {
        self.skills.len()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}
