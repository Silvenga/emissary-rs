use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::tag,
    character::complete::{char, digit1},
    combinator::{eof, map, map_res},
    multi::many0,
};
use tracing::warn;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ServiceLabel {
    pub service_name: String,
    pub port: Option<u16>,
    pub tags: Vec<String>,
}

impl ServiceLabel {
    /// Checks if a label key is an Emissary service label.
    pub fn is_emissary_label(key: &str) -> bool {
        key.starts_with("com.silvenga.emissary.service")
    }

    /// Parses all Emissary service labels from a map.
    pub fn from_labels(labels: &std::collections::HashMap<String, String>) -> Vec<Self> {
        labels
            .iter()
            .filter(|(k, _)| Self::is_emissary_label(k))
            .filter_map(|(k, v)| match Self::parse(v) {
                Ok((_, sl)) => Some(sl),
                Err(_) => {
                    warn!(
                        "Skipping invalid service label on key '{}': value '{}' is malformed.",
                        k, v
                    );
                    None
                }
            })
            .collect()
    }

    /// Parses a label value into a ServiceLabel.
    /// Format: service-name[;port][;tags=tag1,tag2]
    pub fn parse(input: &str) -> IResult<&str, Self> {
        let (input, service_name) = parse_name(input)?;
        let (input, components) = many0(parse_component).parse(input)?;
        let (input, _) = eof(input)?;

        let mut port = None;
        let mut tags = Vec::new();

        for comp in components {
            match comp {
                Component::Port(p) => port = Some(p),
                Component::Tags(t) => tags = t,
            }
        }

        Ok((
            input,
            ServiceLabel {
                service_name,
                port,
                tags,
            },
        ))
    }
}

fn is_separator(c: char) -> bool {
    c == ';' || c == '.'
}

fn separator(input: &str) -> IResult<&str, char> {
    alt((char(';'), char('.'))).parse(input)
}

fn is_dns_safe(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

fn parse_name(input: &str) -> IResult<&str, String> {
    map(nom::bytes::complete::take_while1(is_dns_safe), |s: &str| {
        s.to_owned()
    })
    .parse(input)
}

fn parse_port_part(input: &str) -> IResult<&str, u16> {
    map_res(digit1, |s: &str| s.parse::<u16>()).parse(input)
}

fn parse_tags_part(input: &str) -> IResult<&str, Vec<String>> {
    let (input, _) = tag("tags=").parse(input)?;
    let (input, tags_str) =
        nom::bytes::complete::take_while(|c: char| !is_separator(c)).parse(input)?;
    let tags = tags_str
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    Ok((input, tags))
}

#[derive(Debug)]
enum Component {
    Port(u16),
    Tags(Vec<String>),
}

fn parse_component(input: &str) -> IResult<&str, Component> {
    let (input, _) = separator(input)?;
    alt((
        map(parse_tags_part, Component::Tags),
        map(parse_port_part, Component::Port),
    ))
    .parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn when_parsing_full_label_then_it_should_return_all_fields() {
        let input = "web;8080;tags=a,b";

        let (_, config) = ServiceLabel::parse(input).unwrap();

        assert_eq!(config.service_name, "web");
        assert_eq!(config.port, Some(8080));
        assert_eq!(config.tags, vec!["a", "b"]);
    }

    #[test]
    fn when_parsing_label_with_dots_then_it_should_succeed() {
        let input = "web.8080.tags=prod";

        let (_, config) = ServiceLabel::parse(input).unwrap();

        assert_eq!(config.service_name, "web");
        assert_eq!(config.port, Some(8080));
        assert_eq!(config.tags, vec!["prod"]);
    }

    #[test]
    fn when_parsing_label_without_port_then_it_should_have_none() {
        let input = "web;tags=a,b";

        let (_, config) = ServiceLabel::parse(input).unwrap();

        assert_eq!(config.service_name, "web");
        assert_eq!(config.port, None);
        assert_eq!(config.tags, vec!["a", "b"]);
    }

    #[test]
    fn when_parsing_minimal_label_then_it_should_succeed() {
        let input = "web";

        let (_, config) = ServiceLabel::parse(input).unwrap();

        assert_eq!(config.service_name, "web");
        assert_eq!(config.port, None);
        assert_eq!(config.tags, Vec::<String>::new());
    }

    #[test]
    fn when_parsing_label_with_reordered_components_then_it_should_succeed() {
        let input = "web;tags=a,b;8080";

        let (_, config) = ServiceLabel::parse(input).unwrap();

        assert_eq!(config.service_name, "web");
        assert_eq!(config.port, Some(8080));
        assert_eq!(config.tags, vec!["a", "b"]);
    }

    #[test]
    fn when_parsing_multiple_labels_then_it_should_return_all_services() {
        use std::collections::HashMap;
        let mut labels = HashMap::new();
        labels.insert(
            "com.silvenga.emissary.service.web".to_owned(),
            "web;80".to_owned(),
        );
        labels.insert(
            "com.silvenga.emissary.service.api".to_owned(),
            "api;8080".to_owned(),
        );
        labels.insert("other.label".to_owned(), "value".to_owned());

        let services = ServiceLabel::from_labels(&labels);

        assert_eq!(services.len(), 2);
        assert!(services.iter().any(|s| s.service_name == "web"));
        assert!(services.iter().any(|s| s.service_name == "api"));
    }

    #[test]
    fn when_parsing_label_with_trailing_garbage_then_it_should_fail() {
        let input = "web;80;garbage";

        let result = ServiceLabel::parse(input);

        assert!(result.is_err());
    }

    #[test]
    fn when_parsing_label_with_trailing_garbage_after_tags_then_it_should_fail() {
        let input = "web;80;tags=a,b;garbage";

        let result = ServiceLabel::parse(input);

        assert!(result.is_err());
    }

    #[test]
    fn when_parsing_label_with_url_path_injection_hash_then_it_should_fail() {
        let input = "web#evil;80";

        let result = ServiceLabel::parse(input);

        assert!(result.is_err());
    }

    #[test]
    fn when_parsing_label_with_url_path_injection_question_then_it_should_fail() {
        let input = "web?evil;80";

        let result = ServiceLabel::parse(input);

        assert!(result.is_err());
    }

    #[test]
    fn when_parsing_label_with_space_in_name_then_it_should_fail() {
        let input = "web site;80";

        let result = ServiceLabel::parse(input);

        assert!(result.is_err());
    }

    #[test]
    fn when_parsing_label_with_underscore_in_name_then_it_should_fail() {
        let input = "my_service;80";

        let result = ServiceLabel::parse(input);

        assert!(result.is_err());
    }

    #[test]
    fn when_parsing_label_with_hyphen_in_name_then_it_should_succeed() {
        let input = "my-service;80";

        let (_, config) = ServiceLabel::parse(input).unwrap();

        assert_eq!(config.service_name, "my-service");
        assert_eq!(config.port, Some(80));
    }

    #[test]
    fn when_parsing_label_with_empty_name_then_it_should_fail() {
        let input = ";80";

        let result = ServiceLabel::parse(input);

        assert!(result.is_err());
    }

    #[test]
    fn when_parsing_label_with_dot_in_name_then_it_should_treat_dot_as_separator() {
        let input = "web.8080";

        let (_, config) = ServiceLabel::parse(input).unwrap();

        assert_eq!(config.service_name, "web");
        assert_eq!(config.port, Some(8080));
    }

    #[test]
    fn when_parsing_label_with_uppercase_in_name_then_it_should_succeed() {
        let input = "MyService;80";

        let (_, config) = ServiceLabel::parse(input).unwrap();

        assert_eq!(config.service_name, "MyService");
    }

    #[test]
    fn when_parsing_label_with_digits_in_name_then_it_should_succeed() {
        let input = "web123;80";

        let (_, config) = ServiceLabel::parse(input).unwrap();

        assert_eq!(config.service_name, "web123");
    }

    #[test]
    fn when_from_labels_receives_invalid_label_then_it_should_drop_it() {
        use std::collections::HashMap;
        let mut labels = HashMap::new();
        labels.insert(
            "com.silvenga.emissary.service.web".to_owned(),
            "web;80".to_owned(),
        );
        labels.insert(
            "com.silvenga.emissary.service.bad".to_owned(),
            "my_service;80".to_owned(),
        );

        let services = ServiceLabel::from_labels(&labels);

        assert_eq!(services.len(), 1);
        assert!(services.iter().any(|s| s.service_name == "web"));
    }
}
