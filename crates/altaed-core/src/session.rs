use crate::document::CodeDocument;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Session {
    pub documents: Vec<CodeDocument>,
    pub active: usize,
}
impl Session {
    pub fn ensure_document(&mut self) {
        if self.documents.is_empty() {
            self.documents.push(CodeDocument::untitled());
        }
        self.active = self.active.min(self.documents.len() - 1);
        let mut used = self
            .documents
            .iter()
            .filter_map(CodeDocument::untitled_number)
            .collect();
        for (index, document) in self.documents.iter_mut().enumerate() {
            document.ensure_untitled_number(&used, index as u64 + 1);
            used.extend(document.untitled_number());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_untitled_numbers_survive_reordering_and_new_documents_do_not_reuse_them() {
        let mut saved = CodeDocument::untitled();
        saved.path = Some("main.cpp".into());
        let session = Session {
            documents: vec![saved, CodeDocument::untitled(), CodeDocument::untitled()],
            active: 1,
        };
        let mut legacy = serde_json::to_value(session).unwrap();
        for document in legacy["documents"].as_array_mut().unwrap() {
            document.as_object_mut().unwrap().remove("untitled_number");
        }
        let mut restored: Session = serde_json::from_value(legacy).unwrap();
        restored.ensure_document();
        assert_eq!(restored.documents[1].display_name(), "未命名 2");
        assert_eq!(restored.documents[2].display_name(), "未命名 3");
        restored.documents.swap(0, 2);
        restored.ensure_document();
        assert_eq!(restored.documents[0].display_name(), "未命名 3");
        let used = restored
            .documents
            .iter()
            .filter_map(CodeDocument::untitled_number)
            .collect();
        let mut next = CodeDocument::untitled();
        next.ensure_untitled_number(&used, 3);
        assert_ne!(next.display_name(), "未命名 3");
        assert_ne!(next.display_name(), "未命名 2");
    }
}
