pub async fn embed(base_url: &str, api_key: &str, model: &str, dimension: usize, contents: serde_json::Value) -> Result<Vec<f32>, String> {
    let base = base_url.trim_end_matches('/');
    let compatible = base.contains("/compatible-mode/");
    let url = if compatible { format!("{base}/embeddings") } else { format!("{base}/services/embeddings/multimodal-embedding/multimodal-embedding") };
    let body = if compatible {
        let text = contents.as_array().and_then(|items| items.first()).and_then(|item| item.get("text")).and_then(|text| text.as_str())
            .ok_or("兼容接口目前只支持文本，请使用原生多模态 URL 处理图片")?;
        serde_json::json!({ "model": model, "input": text, "dimensions": dimension })
    } else {
        serde_json::json!({ "model": model, "input": { "contents": contents }, "parameters": { "dimension": dimension, "enable_fusion": true } })
    };
    let response = reqwest::Client::new().post(url).timeout(std::time::Duration::from_secs(90)).bearer_auth(api_key).json(&body).send().await.map_err(|e| format!("网络请求失败：{e}"))?;
    let status = response.status();
    let text = response.text().await.map_err(|e| format!("读取 Embedding 响应失败（HTTP {status}）：{e}"))?;
    let body: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("Embedding 响应不是有效 JSON（HTTP {status}）：{e}"))?;
    if !status.is_success() {
        let message = body.get("message").and_then(serde_json::Value::as_str)
            .or_else(|| body.get("code").and_then(serde_json::Value::as_str))
            .unwrap_or(text.as_str());
        return Err(format!("Embedding 请求失败（HTTP {status}）：{message}"));
    }
    let embeddings = body.get("data").or_else(|| body.get("output").and_then(|value| value.get("embeddings"))).and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("Embedding 响应缺少 output.embeddings：{text}"))?;
    embeddings.first().and_then(|item| item.get("embedding")).and_then(serde_json::Value::as_array)
        .map(|values| values.iter().filter_map(serde_json::Value::as_f64).map(|value| value as f32).collect())
        .ok_or_else(|| format!("Embedding 响应缺少向量：{text}"))
}
