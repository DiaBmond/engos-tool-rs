use crate::application::roleplay::dto::{RoleplayScenario, RoleplayReply, RoleplayEvaluation};
use crate::application::roleplay::ports::RoleplayAiPort;
use super::client::GeminiClient;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GeminiScenarioResponse {
    role_name: String,
    setting: String,
    opening_line: String,
}

#[derive(Debug, Deserialize)]
struct GeminiReplyResponse {
    ai_message: String,
    is_understood: bool,
    hint: String,
}

#[derive(Debug, Deserialize)]
struct GeminiEvalResponse {
    is_passed: bool,
    summary_feedback: String,
}

impl RoleplayAiPort for GeminiClient {
    async fn generate_scenario(&self, level: u8) -> Result<RoleplayScenario, String> {
        let difficulty_desc = match level {
            1 => "Level 1 (Beginner): Simple daily life situations such as ordering food, asking for directions, or shopping (use basic vocabulary and short sentences).",
            2 => "Level 2 (Intermediate): Workplace or travel situations requiring problem-solving, such as checking into a fully booked hotel or discussing a task with a foreign colleague.",
            3 => "Level 3 (Advanced): High-confidence situations requiring explanation and negotiation, such as a Tech job interview, negotiating with a client, or presenting a project.",
            4 => "Level 4 (Native/Master): High-pressure crisis management, such as explaining a Production Outage to executives or resolving a business dispute.",
            _ => "Level 1 (Beginner): General daily life situations.",
        };

        let prompt = format!(
            r#"You are an English Roleplay AI Director.
            Generate ONE English conversation roleplay scenario with the following difficulty:
            "{}"

            Respond STRICTLY as a JSON object with these keys:
            - "role_name": Name and persona of the character the AI will play (e.g., "John, an angry senior developer").
            - "setting": A clear explanation of the context in Thai, explicitly stating who the "User" plays as and their objective to succeed in this roleplay.
            - "opening_line": The very first opening line from the AI character to the user in English, staying perfectly in character."#,
            difficulty_desc
        );

        let parsed: GeminiScenarioResponse = self
            .generate_json(
                Some("You are an expert English roleplay director."),
                &prompt,
            )
            .await?;

        Ok(RoleplayScenario {
            role_name: parsed.role_name,
            setting: parsed.setting,
            opening_line: parsed.opening_line,
        })
    }

    async fn respond_in_character(
        &self,
        scenario: &RoleplayScenario,
        chat_history: &[(String, String)],
        user_message: &str,
    ) -> Result<RoleplayReply, String> {
        let mut history_text = String::new();
        for (u_msg, ai_msg) in chat_history {
            history_text.push_str(&format!("User: {}\nAI ({}): {}\n", u_msg, scenario.role_name, ai_msg));
        }

        let prompt = format!(
            r#"You are currently roleplaying as "{}" in the following setting: "{}"
            
            Conversation History:
            {}
            User just said: "{}"

            Respond naturally and authentically in character as a native English speaker.
            Respond STRICTLY as a JSON object with these keys:
            - "ai_message": Your reply in English (DO NOT break character).
            - "is_understood": Boolean (true if the user's English input is comprehensible in context, false if completely gibberish or completely out of context).
            - "hint": A short, useful tip or suggested vocabulary in Thai to hint at how the user could answer next or what sentence structure they could use."#,
            scenario.role_name, scenario.setting, history_text, user_message
        );

        let parsed: GeminiReplyResponse = self
            .generate_json(
                Some("You are a talented actor and English teacher roleplaying a character."),
                &prompt,
            )
            .await?;

        Ok(RoleplayReply {
            ai_message: parsed.ai_message,
            is_understood: parsed.is_understood,
            hint: Some(parsed.hint),
        })
    }

    async fn evaluate_session(
        &self,
        scenario: &RoleplayScenario,
        chat_history: &[(String, String)],
    ) -> Result<RoleplayEvaluation, String> {
        let mut history_text = String::new();
        for (u_msg, ai_msg) in chat_history {
            history_text.push_str(&format!("User: {}\nAI: {}\n", u_msg, ai_msg));
        }

        let prompt = format!(
            r#"You are an English Communication Evaluator.
            Please review this entire roleplay session "{}" :
            {}

            Evaluate whether the user communicated effectively, achieved the objective of the scenario, and used appropriate grammar:
            - If the user communicated well, was understandable, and successfully survived/resolved the situation (even with minor grammatical mistakes that did not break meaning): set "is_passed": true.
            - If communication was impossible, words were severely misused, or the user completely failed the objective: set "is_passed": false.

            Respond STRICTLY as a JSON object with these keys:
            - "is_passed": Boolean (true if passed, false if failed).
            - "summary_feedback": Constructive feedback in Thai summarizing strengths and specific areas to improve to level up next time."#,
            scenario.setting, history_text
        );

        let parsed: GeminiEvalResponse = self
            .generate_json(
                Some("You are a strict but encouraging English language evaluator."),
                &prompt,
            )
            .await?;

        Ok(RoleplayEvaluation {
            is_passed: parsed.is_passed,
            summary_feedback: parsed.summary_feedback,
        })
    }
}