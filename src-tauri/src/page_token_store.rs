use std::collections::HashMap;
use std::sync::Mutex;

pub struct PageTokenStore(pub Mutex<HashMap<String, String>>);

impl PageTokenStore {
    pub fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.0.lock().unwrap().get(key).cloned()
    }

    pub fn set(&self, key: &str, token: String) {
        self.0.lock().unwrap().insert(key.to_string(), token);
    }

    pub fn remove(&self, key: &str) {
        self.0.lock().unwrap().remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get() {
        let store = PageTokenStore::new();
        store.set("key1", "token1".to_string());
        assert_eq!(store.get("key1"), Some("token1".to_string()));
    }

    #[test]
    fn test_get_missing() {
        let store = PageTokenStore::new();
        assert_eq!(store.get("missing"), None);
    }

    #[test]
    fn test_remove() {
        let store = PageTokenStore::new();
        store.set("key1", "token1".to_string());
        store.remove("key1");
        assert_eq!(store.get("key1"), None);
    }

    #[test]
    fn test_overwrite() {
        let store = PageTokenStore::new();
        store.set("key1", "token1".to_string());
        store.set("key1", "token2".to_string());
        assert_eq!(store.get("key1"), Some("token2".to_string()));
    }
}
