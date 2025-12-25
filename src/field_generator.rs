use std::collections::HashMap;
use uuid::Uuid;

use crate::config::GeneratedField;

/// 字段生成器
pub struct FieldGenerator;

impl FieldGenerator {
    /// 根据配置生成字段值
    pub fn generate_field(field: &GeneratedField, cycle: usize) -> String {
        match field.generator.as_str() {
            "random" => Self::generate_random(cycle),
            "timestamp" => Self::generate_timestamp(cycle),
            "counter" => Self::generate_counter(cycle),
            "uuid" => Self::generate_uuid(),
            "fixed" => field.value.clone().unwrap_or_else(|| "default".to_string()),
            _ => field.value.clone().unwrap_or_else(|| "unknown".to_string()),
        }
    }

    /// 分离字段类型（header vs body）
    pub fn separate_fields_by_type(
        generated_fields: &Option<Vec<GeneratedField>>,
        cycle: usize,
    ) -> (HashMap<String, String>, HashMap<String, String>) {
        let mut header_fields = HashMap::new();
        let mut body_fields = HashMap::new();

        if let Some(field_configs) = generated_fields {
            for field_config in field_configs {
                let value = Self::generate_field(field_config, cycle);
                if field_config.field_type == "body" {
                    body_fields.insert(field_config.name.clone(), value);
                } else {
                    header_fields.insert(field_config.name.clone(), value);
                }
            }
        }

        (header_fields, body_fields)
    }

    /// 生成动态body内容
    pub fn generate_dynamic_body(
        base_body: &Option<String>,
        body_fields: &HashMap<String, String>,
    ) -> Option<String> {
        if let Some(body) = base_body {
            let mut dynamic_body = body.clone();

            // 替换body中的占位符为生成的值
            for (field_name, field_value) in body_fields {
                let placeholder = format!("{{{}}}", field_name);
                dynamic_body = dynamic_body.replace(&placeholder, field_value);
            }

            Some(dynamic_body)
        } else {
            // 如果没有基础body，创建一个包含body字段的JSON对象
            if !body_fields.is_empty() {
                let mut json_body = String::from("{");
                let mut first = true;
                for (key, value) in body_fields {
                    if !first {
                        json_body.push(',');
                    }
                    json_body.push_str(&format!("\"{}\":\"{}\"", key, value));
                    first = false;
                }
                json_body.push('}');
                Some(json_body)
            } else {
                None
            }
        }
    }

    /// 生成随机值
    fn generate_random(cycle: usize) -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let random_num: u32 = rng.gen_range(1000..9999);
        format!("random_{}_{}", cycle, random_num)
    }

    /// 生成时间戳
    fn generate_timestamp(cycle: usize) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        format!("timestamp_{}_{}", cycle, now)
    }

    /// 生成计数器值
    fn generate_counter(cycle: usize) -> String {
        format!("counter_{}", cycle)
    }

    /// 生成UUID
    fn generate_uuid() -> String {
        Uuid::new_v4().to_string()
    }
}
