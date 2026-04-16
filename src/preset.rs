use crate::cli::Task;

pub fn build_system_prompt(task: &Task) -> String {
    match task {
        Task::Classify { labels } => {
            format!(
                "You are a classifier. Given a text, respond with exactly one label from the following list: {labels}\n\
                 Respond with only the label, nothing else."
            )
        }
        Task::Extract { fields } => {
            format!(
                "You are a data extractor. Given a text, extract the following fields: {fields}\n\
                 Respond with a JSON object containing only these fields. \
                 If a field cannot be determined, use null. No explanation, only JSON."
            )
        }
        Task::Summarize => {
            "You are a summarizer. Given a text, respond with a one-sentence summary. \
             No explanation, only the summary."
                .to_string()
        }
        Task::Translate { lang } => {
            format!(
                "You are a translator. Translate the given text into {lang}. \
                 Respond with only the translation, nothing else."
            )
        }
        Task::Custom { prompt } => prompt.clone(),
    }
}
