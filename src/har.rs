use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Har {
    pub log: Log,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Log {
    pub version: String,
    pub creator: Creator,
    pub entries: Vec<Entry>,
    pub pages: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Creator {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Entry {
    #[serde(rename = "startedDateTime")]
    pub started_date_time: String,
    pub time: f64,
    pub request: Request,
    pub response: Response,
    pub timings: Timings,
    pub cache: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Request {
    pub method: String,
    pub url: String,
    #[serde(rename = "httpVersion")]
    pub http_version: String,
    pub headers: Vec<Header>,
    #[serde(rename = "queryString")]
    pub query_string: Vec<QueryParam>,
    #[serde(rename = "headersSize")]
    pub headers_size: i64,
    #[serde(rename = "bodySize")]
    pub body_size: i64,
    #[serde(rename = "postData")]
    pub post_data: Option<PostData>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Response {
    pub status: u16,
    #[serde(rename = "statusText")]
    pub status_text: String,
    #[serde(rename = "httpVersion")]
    pub http_version: String,
    pub headers: Vec<Header>,
    pub content: Content,
    #[serde(rename = "redirectURL")]
    pub redirect_url: String,
    #[serde(rename = "headersSize")]
    pub headers_size: i64,
    #[serde(rename = "bodySize")]
    pub body_size: i64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct QueryParam {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PostData {
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Content {
    pub size: i64,
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Timings {
    pub send: f64,
    pub wait: f64,
    pub receive: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_valid_har() {
        let json = include_str!("../tests/fixtures/valid.har");
        let har: Har = serde_json::from_str(json).unwrap();
        assert_eq!(har.log.version, "1.2");
        assert_eq!(har.log.entries.len(), 4);

        let first = &har.log.entries[0];
        assert_eq!(first.request.method, "GET");
        assert_eq!(first.request.url, "https://api.example.com/users");
        assert_eq!(first.response.status, 200);
        assert_eq!(first.time, 42.5);
    }

    #[test]
    fn test_deserialize_minimal_har() {
        let json = include_str!("../tests/fixtures/minimal.har");
        let har: Har = serde_json::from_str(json).unwrap();
        assert_eq!(har.log.entries.len(), 0);
    }

    #[test]
    fn test_deserialize_malformed_har_fails() {
        let json = include_str!("../tests/fixtures/malformed.har");
        let result = serde_json::from_str::<Har>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_optional_fields() {
        let json = include_str!("../tests/fixtures/valid.har");
        let har: Har = serde_json::from_str(json).unwrap();

        // First entry has no postData
        assert!(har.log.entries[0].request.post_data.is_none());
        // Second entry has postData
        assert!(har.log.entries[1].request.post_data.is_some());
        assert_eq!(
            har.log.entries[1].request.post_data.as_ref().unwrap().text,
            Some("{\"name\": \"Alice\"}".to_string())
        );
    }
}
