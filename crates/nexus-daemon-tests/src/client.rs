use reqwest::StatusCode;
use serde_json::Value;

pub struct DaemonClient {
    inner: reqwest::Client,
    base_url: String,
}

impl DaemonClient {
    pub fn new(base_url: String) -> Self {
        Self {
            inner: reqwest::Client::new(),
            base_url,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    pub async fn get(&self, path: &str) -> (StatusCode, Value) {
        let resp = self.inner.get(self.url(path)).send().await.expect("GET failed");
        let status = resp.status();
        let body = resp.json::<Value>().await.unwrap_or(Value::Null);
        (status, body)
    }

    pub async fn post<B: serde::Serialize>(&self, path: &str, body: &B) -> (StatusCode, Value) {
        let resp = self
            .inner
            .post(self.url(path))
            .json(body)
            .send()
            .await
            .expect("POST failed");
        let status = resp.status();
        let body = resp.json::<Value>().await.unwrap_or(Value::Null);
        (status, body)
    }

    pub async fn post_empty(&self, path: &str) -> (StatusCode, Value) {
        self.post(path, &serde_json::json!({})).await
    }

    pub async fn put<B: serde::Serialize>(&self, path: &str, body: &B) -> (StatusCode, Value) {
        let resp = self
            .inner
            .put(self.url(path))
            .json(body)
            .send()
            .await
            .expect("PUT failed");
        let status = resp.status();
        let body = resp.json::<Value>().await.unwrap_or(Value::Null);
        (status, body)
    }

    pub async fn patch<B: serde::Serialize>(&self, path: &str, body: &B) -> (StatusCode, Value) {
        let resp = self
            .inner
            .patch(self.url(path))
            .json(body)
            .send()
            .await
            .expect("PATCH failed");
        let status = resp.status();
        let body = resp.json::<Value>().await.unwrap_or(Value::Null);
        (status, body)
    }

    pub async fn delete(&self, path: &str) -> (StatusCode, Value) {
        let resp = self
            .inner
            .delete(self.url(path))
            .send()
            .await
            .expect("DELETE failed");
        let status = resp.status();
        if status == StatusCode::NO_CONTENT {
            return (status, Value::Null);
        }
        let body = resp.json::<Value>().await.unwrap_or(Value::Null);
        (status, body)
    }
}
