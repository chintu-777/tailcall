#![allow(clippy::too_many_arguments)]

use derive_setters::Setters;
use hyper::HeaderMap;
use reqwest::header::{HeaderName, HeaderValue};

use crate::config::{GraphQLOperationType, KeyValues};
use crate::has_headers::HasHeaders;
use crate::http::Method::POST;
use crate::lambda::GraphQLOperationContext;
use crate::mustache::Mustache;
use crate::path_string::PathGraphql;

/// RequestTemplate for GraphQL requests (See RequestTemplate documentation)
#[derive(Setters, Debug, Clone)]
pub struct GraphqlRequestTemplate {
  pub url: String,
  pub operation_type: GraphQLOperationType,
  pub operation_name: String,
  pub operation_arguments: Option<Vec<(String, Mustache)>>,
  pub headers: Vec<(HeaderName, Mustache)>,
}

impl GraphqlRequestTemplate {
  fn create_headers<C: PathGraphql>(&self, ctx: &C) -> HeaderMap {
    let mut header_map = HeaderMap::new();

    for (k, v) in &self.headers {
      if let Ok(header_value) = HeaderValue::from_str(&v.render_graphql(ctx)) {
        header_map.insert(k, header_value);
      }
    }

    header_map
  }

  fn set_headers<C: PathGraphql + HasHeaders>(&self, mut req: reqwest::Request, ctx: &C) -> reqwest::Request {
    let headers = req.headers_mut();
    let config_headers = self.create_headers(ctx);

    if !config_headers.is_empty() {
      headers.extend(config_headers);
    }
    headers.insert(
      reqwest::header::CONTENT_TYPE,
      HeaderValue::from_static("application/json"),
    );
    headers.extend(ctx.headers().to_owned());
    req
  }

  pub fn to_request<C: PathGraphql + HasHeaders + GraphQLOperationContext>(
    &self,
    ctx: &C,
  ) -> anyhow::Result<reqwest::Request> {
    let mut req = reqwest::Request::new(POST.to_hyper(), url::Url::parse(self.url.as_str())?);
    req = self.set_headers(req, ctx);
    req = self.set_body(req, ctx);
    Ok(req)
  }

  fn set_body<C: PathGraphql + HasHeaders + GraphQLOperationContext>(
    &self,
    mut req: reqwest::Request,
    ctx: &C,
  ) -> reqwest::Request {
    let operation_type = &self.operation_type;
    let selection_set = ctx.selection_set().unwrap_or_default();
    let operation = self
      .operation_arguments
      .as_ref()
      .map(|args| {
        args
          .iter()
          .map(|(k, v)| format!(r#"{}: {}"#, k, v.render_graphql(ctx).escape_default()))
          .collect::<Vec<_>>()
          .join(", ")
      })
      .map(|args| format!("{}({})", self.operation_name, args))
      .unwrap_or(self.operation_name.clone());

    let graphql_query = format!(r#"{{ "query": "{operation_type} {{ {operation} {{ {selection_set} }} }}" }}"#);

    req.body_mut().replace(graphql_query.into());
    req
  }

  pub fn new(
    url: String,
    operation_type: &GraphQLOperationType,
    operation_name: &str,
    args: Option<&KeyValues>,
    headers: HeaderMap<HeaderValue>,
  ) -> anyhow::Result<Self> {
    let mut operation_arguments = None;

    if let Some(args) = args.as_ref() {
      operation_arguments = Some(
        args
          .iter()
          .map(|(k, v)| Ok((k.to_owned(), Mustache::parse(v)?)))
          .collect::<anyhow::Result<Vec<_>>>()?,
      );
    }

    let headers = headers
      .iter()
      .map(|(k, v)| Ok((k.clone(), Mustache::parse(v.to_str()?)?)))
      .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(Self {
      url,
      operation_type: operation_type.to_owned(),
      operation_name: operation_name.to_owned(),
      operation_arguments,
      headers,
    })
  }
}

#[cfg(test)]
mod tests {
  use hyper::HeaderMap;
  use pretty_assertions::assert_eq;
  use serde_json::json;

  use crate::config::GraphQLOperationType;
  use crate::graphql_request_template::GraphqlRequestTemplate;
  use crate::has_headers::HasHeaders;
  use crate::lambda::GraphQLOperationContext;
  use crate::path_string::PathGraphql;

  struct Context {
    pub value: serde_json::Value,
    pub headers: HeaderMap,
  }

  impl PathGraphql for Context {
    fn path_graphql<T: AsRef<str>>(&self, path: &[T]) -> Option<String> {
      self.value.path_graphql(path)
    }
  }

  impl HasHeaders for Context {
    fn headers(&self) -> &HeaderMap {
      &self.headers
    }
  }

  impl GraphQLOperationContext for Context {
    fn selection_set(&self) -> Option<String> {
      Some("a,b,c".to_owned())
    }
  }

  #[test]
  fn test_query_without_args() {
    let tmpl = GraphqlRequestTemplate::new(
      "http://localhost:3000".to_string(),
      &GraphQLOperationType::Query,
      "myQuery",
      None,
      HeaderMap::new(),
    )
    .unwrap();
    let ctx = Context {
      value: json!({
        "foo": {
          "bar": "baz",
          "header": "abc"
        }
      }),
      headers: Default::default(),
    };

    let req = tmpl.to_request(&ctx).unwrap();
    let body = req.body().unwrap().as_bytes().unwrap().to_owned();

    assert_eq!(
      std::str::from_utf8(&body).unwrap(),
      r#"{ "query": "query { myQuery { a,b,c } }" }"#
    );
  }

  #[test]
  fn test_query_with_args() {
    let tmpl = GraphqlRequestTemplate::new(
      "http://localhost:3000".to_string(),
      &GraphQLOperationType::Mutation,
      "create",
      Some(serde_json::from_str(r#"[{"key": "id", "value": "{{foo.bar}}"}]"#).unwrap()).as_ref(),
      HeaderMap::new(),
    )
    .unwrap();
    let ctx = Context {
      value: json!({
        "foo": {
          "bar": "baz",
          "header": "abc"
        }
      }),
      headers: Default::default(),
    };

    let req = tmpl.to_request(&ctx).unwrap();
    let body = req.body().unwrap().as_bytes().unwrap().to_owned();

    assert_eq!(
      std::str::from_utf8(&body).unwrap(),
      r#"{ "query": "mutation { create(id: \"baz\") { a,b,c } }" }"#
    );
  }
}
