use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Character {
    pub name: String,
    pub preamble: String,
    // pub lore: Vec<String>,
    // pub message_examples: Vec<Vec<Message>>,
    // pub post_examples: Vec<String>,
    // pub topics: Vec<String>,
    // pub style: Style,
    // pub adjectives: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub user: String,
    pub content: MessageContent,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageContent {
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Style {
    pub all: Vec<String>,
    pub chat: Vec<String>,
    pub post: Vec<String>,
}
