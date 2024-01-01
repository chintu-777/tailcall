use std::sync::{Arc, Mutex};

use super::base::{AuthError, AuthProvider};
use super::jwt::JwtProvider;
use crate::blueprint::Blueprint;
use crate::http::RequestContext;
use crate::valid::Valid;

#[derive(Default)]
pub struct GlobalAuthContext {
  // TODO: remove pub and create it from directive
  pub jwt_provider: Option<JwtProvider>,
}

#[derive(Default)]
pub struct AuthContext {
  // TODO: can we do without mutex?
  auth_result: Mutex<Option<Valid<(), AuthError>>>,
  global_ctx: Arc<GlobalAuthContext>,
}

impl GlobalAuthContext {
  async fn validate(&self, request: &RequestContext) -> Valid<(), AuthError> {
    if let Some(jwt_provider) = &self.jwt_provider {
      return jwt_provider.validate(request).await;
    }

    Valid::succeed(())
  }
}

impl From<&Blueprint> for GlobalAuthContext {
  fn from(blueprint: &Blueprint) -> Self {
    let auth = &blueprint.auth;
    let jwt_provider = auth
      .jwt
      .as_ref()
      // the actual validation happens here src/blueprint/from_config/auth.rs, so just .ok() to resolve options
      .and_then(|jwt| JwtProvider::parse(jwt.clone()).to_result().ok());

    Self { jwt_provider }
  }
}

impl AuthContext {
  pub async fn validate(&self, request: &RequestContext) -> Valid<(), AuthError> {
    if let Some(valid) = self.auth_result.lock().unwrap().as_ref() {
      return valid.clone();
    }

    let result = self.global_ctx.validate(request).await;

    self.auth_result.lock().unwrap().replace(result.clone());

    result
  }
}

impl From<&Arc<GlobalAuthContext>> for AuthContext {
  fn from(global_ctx: &Arc<GlobalAuthContext>) -> Self {
    Self { global_ctx: global_ctx.clone(), auth_result: Default::default() }
  }
}