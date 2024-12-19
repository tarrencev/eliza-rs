use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub enum Source {
    Discord,
    Telegram,
    Github,
    X,
    Twitter,
}

impl Source {
    pub fn as_str(&self) -> &'static str {
        match self {
            Source::Discord => "discord",
            Source::Telegram => "telegram",
            Source::Github => "github",
            Source::X => "x",
            Source::Twitter => "twitter",
        }
    }
}

impl FromStr for Source {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "discord" => Ok(Source::Discord),
            "telegram" => Ok(Source::Telegram),
            "github" => Ok(Source::Github),
            "x" => Ok(Source::X),
            "twitter" => Ok(Source::Twitter),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub enum ChannelType {
    DirectMessage,
    Text,
    Voice,
    Thread,
}

impl ChannelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChannelType::DirectMessage => "direct_message",
            ChannelType::Text => "text",
            ChannelType::Voice => "voice",
            ChannelType::Thread => "thread",
        }
    }
}

impl FromStr for ChannelType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "direct_message" => Ok(ChannelType::DirectMessage),
            "text" => Ok(ChannelType::Text),
            "voice" => Ok(ChannelType::Voice),
            "thread" => Ok(ChannelType::Thread),
            _ => Err(()),
        }
    }
}

pub trait MessageMetadata {
    fn id(&self) -> String;
    fn source_id(&self) -> String;
    fn channel_id(&self) -> String;
    fn created_at(&self) -> chrono::DateTime<chrono::Utc>;
    fn source(&self) -> Source;
    fn channel_type(&self) -> ChannelType;
}

pub trait MessageContent {
    fn content(&self) -> &str;
}
