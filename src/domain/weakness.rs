#[derive(Debug, Clone, PartialEq)]
pub enum WeaknessTopic {
    Grammar,
    Vocabulary,
    Preposition,
    Tense,
    Other(String),
}

#[derive(Debug, Clone)]
pub struct Weakness {
    pub weakness_id: String,
    pub user_id: String,
    pub topic: WeaknessTopic,
    pub description: String,
    pub resolved: bool,
}

impl Weakness {
    pub fn new(weakness_id: String, user_id: String, topic: WeaknessTopic, description: String) -> Self{
        Self{
            weakness_id,
            user_id,
            topic,
            description,
            resolved: false,
        }
    }
    
    pub fn mark_as_resolved(&mut self){
        self.resolved = true;
    }
}