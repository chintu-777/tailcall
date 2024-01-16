mod decoder;
mod jwks;
mod remote_jwks;
mod validation;

use std::sync::Arc;

use headers::authorization::Bearer;
use headers::{Authorization, HeaderMapExt};
use serde::Deserialize;

use self::decoder::JwksDecoder;
use self::validation::{validate_aud, validate_iss};
use super::base::{AuthError, AuthProviderTrait};
use crate::http::RequestContext;
use crate::{blueprint, HttpIO};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
  One(T),
  Vec(Vec<T>),
}

#[derive(Debug, Default, Deserialize)]
pub struct JwtClaims {
  aud: Option<OneOrMany<String>>,
  iss: Option<String>,
}

pub struct JwtProvider {
  options: blueprint::JwtProvider,
  decoder: JwksDecoder,
}

impl JwtProvider {
  pub fn new(options: blueprint::JwtProvider, client: Arc<dyn HttpIO>) -> Self {
    Self { decoder: JwksDecoder::new(&options, client), options }
  }

  fn resolve_token(&self, request: &RequestContext) -> Option<String> {
    let value = request.req_headers.typed_get::<Authorization<Bearer>>();

    value.map(|token| token.token().to_owned())
  }

  async fn validate_token(&self, token: &str) -> Result<(), AuthError> {
    let claims = self
      .decoder
      .decode(token)
      .await
      .map_err(|_| AuthError::ValidationCheckFailed)?;

    self.validate_claims(&claims)
  }

  fn validate_claims(&self, claims: &JwtClaims) -> Result<(), AuthError> {
    if !validate_iss(&self.options, claims) || !validate_aud(&self.options, claims) {
      return Err(AuthError::Invalid);
    }

    Ok(())
  }
}

impl AuthProviderTrait for JwtProvider {
  async fn validate(&self, request: &RequestContext) -> Result<(), AuthError> {
    let token = self.resolve_token(request);

    let Some(token) = token else {
      return Err(AuthError::Missing);
    };

    self.validate_token(&token).await
  }
}

#[cfg(test)]
pub mod tests {
  use std::collections::HashSet;

  use jsonwebtoken::jwk::JwkSet;
  use once_cell::sync::Lazy;

  use super::*;
  use crate::http::Response;

  struct MockHttpClient;

  #[async_trait::async_trait]
  impl HttpIO for MockHttpClient {
    async fn execute(&self, _request: reqwest::Request) -> anyhow::Result<Response<hyper::body::Bytes>> {
      todo!()
    }
  }

  // tokens are valid for 10 years. If it is expired, update it =)
  // to parse the token and see its content use https://jwt.io

  // token with kid, issuer = "me" and audience = ["them"]
  pub const JWT_VALID_TOKEN_WITH_KID: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6Ikk0OHFNSnA1NjZTU0tRb2dZWFl0SEJvOXE2WmNFS0hpeE5QZU5veFYxYzgifQ.eyJleHAiOjIwMTkwNTY0NDEuMCwiaXNzIjoibWUiLCJzdWIiOiJ5b3UiLCJhdWQiOlsidGhlbSJdfQ.cU-hJgVGWxK3-IBggYBChhf3FzibBKjuDLtq2urJ99FVXIGZls0VMXjyNW7yHhLLuif_9t2N5UIUIq-hwXVv7rrGRPCGrlqKU0jsUH251Spy7_ppG5_B2LsG3cBJcwkD4AVz8qjT3AaE_vYZ4WnH-CQ-F5Vm7wiYZgbdyU8xgKoH85KAxaCdJJlYOi8mApE9_zcdmTNJrTNd9sp7PX3lXSUu9AWlrZkyO-HhVbXFunVtfduDuTeVXxP8iw1wt6171CFbPmQJU_b3xCornzyFKmhSc36yvlDfoPPclWmWeyOfFEp9lVhQm0WhfDK7GiuRtaOxD-tOvpTjpcoZBeJb7bSg2OsneyeM_33a0WoPmjHw8WIxbroJz_PrfE72_TzbcTSDttKAv_e75PE48Vvx0661miFv4Gq8RBzMl2G3pQMEVCOm83v7BpodfN_YVJcqZJjVHMA70TZQ4K3L4_i9sIK9jJFfwEDVM7nsDnUu96n4vKs1fVvAuieCIPAJrfNOUMy7TwLvhnhUARsKnzmtNNrJuDhhBx-X93AHcG3micXgnqkFdKn6-ZUZ63I2KEdmjwKmLTRrv4n4eZKrRN-OrHPI4gLxJUhmyPAHzZrikMVBcDYfALqyki5SeKkwd4v0JAm87QzR4YwMdKErr0Xa5JrZqHGe2TZgVO4hIc-KrPw";

  // token without kid, issuer = "me" and audience = "some"
  pub const JWT_VALID_TOKEN_NO_KID: &str = "eyJhbGciOiJSUzI1NiJ9.eyJleHAiOjIwMTkwNTY0NDEsImlzcyI6Im1lIiwiYXVkIjoic29tZSJ9.E_3s1MCdyRPDvpTtM4woHmSrRxU3_zRMSIbGSQYe3zyRQ-d2Tw6jVVleZ39GJ88l3yw0pGrrkdGkRBi1lammrUryoe0Sp8_FQ-tZ1jrkCV3qd75n3X_WYnG8CRiPaDZX5VDEFlF30h1x3gyEBpDloOa657AYqwG20XTG5xgicvOGY7SGsyO6IwNWXbbiJnH5cStNPb5mQ97cY8QDKryT5InWHWMO1USByqUYoj-AL4HdIrr5HUaZqDIJEberLddIIHW446pd55PhW6PXS9voLmJv9in9ckCTij_AVOdr7shDlQqZhfIZAVYFSqG64Vs4GM1jEwHVoP_EK-4L7nq3TQ";

  pub static JWK_SET: Lazy<JwkSet> = Lazy::new(|| {
    let value = serde_json::json!({
      "keys": [
        {
          "kty": "RSA",
          "use": "sig",
          "alg": "RS256",
          "kid": "I48qMJp566SSKQogYXYtHBo9q6ZcEKHixNPeNoxV1c8",
          "n": "ksMb5oMlhJ_HzAebCuBG6-v5Qc4J111ur7Aux6-8SbxzqFONsf2Bw6ATG8pAfNeZ-USA3_T1mGkYTDvfoggXnxsduWV_lePZKKOq_Qp_EDdzic1bVTJQDad3CXldR3wV6UFDtMx6cCLXxPZM5n76e7ybPt0iNgwoGpJE28emMZJXrnEUFzxwFMq61UlzWEumYqW3uOUVp7r5XAF5jQ_1nQAnpHBnRFzdNPVb3E6odMGu3jgp8mkPbPMP16Fund4LVplLz8yrsE9TdVrSdYJThylRWn_BwvJ0DjUcp8ibJya86iClUlixAmBwR9NdStHwQqHwmMXMKkTXo-ytRmSUobzxX9T8ESkij6iBhQpmDMD3FbkK30Y7pUVEBBOyDfNcWOhholjOj9CRrxu9to5rc2wvufe24VlbKb9wngS_uGfK4AYvVyrcjdYMFkdqw-Mft14HwzdO2BTS0TeMDZuLmYhj_bu5_g2Zu6PH5OpIXF6Fi8_679pCG8wWAcFQrFrM0eA70wD_SqD_BXn6pWRpFXlcRy_7PWTZ3QmC7ycQFR6Wc6Px44y1xDUoq3rH0RlZkeicfvP6FRlpjFU7xF6LjAfd9ciYBZfJll6PE7zf-i_ZXEslv-tJ5-30-I4Slwj0tDrZ2Z54OgAg07AIwAiI5o4y-0vmuhUscNpfZsGAGhE",
          "e": "AQAB"
        },
        {
          "kty": "RSA",
          "n": "u1SU1LfVLPHCozMxH2Mo4lgOEePzNm0tRgeLezV6ffAt0gunVTLw7onLRnrq0_IzW7yWR7QkrmBL7jTKEn5u-qKhbwKfBstIs-bMY2Zkp18gnTxKLxoS2tFczGkPLPgizskuemMghRniWaoLcyehkd3qqGElvW_VDL5AaWTg0nLVkjRo9z-40RQzuVaE8AkAFmxZzow3x-VJYKdjykkJ0iT9wCS0DRTXu269V264Vf_3jvredZiKRkgwlL9xNAwxXFg0x_XFw005UWVRIkdgcKWTjpBP2dPwVZ4WWC-9aGVd-Gyn1o0CLelf4rEjGoXbAAEgAqeGUxrcIlbjXfbcmw",
          "e": "AQAB",
          "alg": "RS256"
        }
      ]
    });

    serde_json::from_value(value).unwrap()
  });

  impl blueprint::JwtProvider {
    pub fn test_value() -> Self {
      let jwks = blueprint::Jwks::Local(JWK_SET.clone());

      Self { issuer: Default::default(), audiences: Default::default(), optional_kid: false, jwks }
    }
  }

  pub fn create_jwt_auth_request(token: &str) -> RequestContext {
    let mut req_context = RequestContext::default();

    req_context
      .req_headers
      .typed_insert(Authorization::bearer(token).unwrap());

    req_context
  }

  #[tokio::test]
  async fn validate_token_iss() {
    let jwt_options = blueprint::JwtProvider::test_value();
    let jwt_provider = JwtProvider::new(jwt_options, Arc::new(MockHttpClient));

    let valid = jwt_provider
      .validate(&create_jwt_auth_request(JWT_VALID_TOKEN_WITH_KID))
      .await;

    assert!(valid.is_ok());

    let jwt_options = blueprint::JwtProvider { issuer: Some("me".to_owned()), ..blueprint::JwtProvider::test_value() };
    let jwt_provider = JwtProvider::new(jwt_options, Arc::new(MockHttpClient));

    let valid = jwt_provider
      .validate(&create_jwt_auth_request(JWT_VALID_TOKEN_WITH_KID))
      .await;

    assert!(valid.is_ok());

    let jwt_options =
      blueprint::JwtProvider { issuer: Some("another".to_owned()), ..blueprint::JwtProvider::test_value() };
    let jwt_provider = JwtProvider::new(jwt_options, Arc::new(MockHttpClient));

    let error = jwt_provider
      .validate(&create_jwt_auth_request(JWT_VALID_TOKEN_WITH_KID))
      .await
      .err();

    assert_eq!(error, Some(AuthError::Invalid));
  }

  #[tokio::test]
  async fn validate_token_aud() {
    let jwt_options = blueprint::JwtProvider::test_value();
    let jwt_provider = JwtProvider::new(jwt_options, Arc::new(MockHttpClient));

    let valid = jwt_provider
      .validate(&create_jwt_auth_request(JWT_VALID_TOKEN_WITH_KID))
      .await;

    assert!(valid.is_ok());

    let jwt_options = blueprint::JwtProvider {
      audiences: HashSet::from_iter(["them".to_string()]),
      ..blueprint::JwtProvider::test_value()
    };
    let jwt_provider = JwtProvider::new(jwt_options, Arc::new(MockHttpClient));

    let valid = jwt_provider
      .validate(&create_jwt_auth_request(JWT_VALID_TOKEN_WITH_KID))
      .await;

    assert!(valid.is_ok());

    let jwt_options = blueprint::JwtProvider {
      audiences: HashSet::from_iter(["anothem".to_string()]),
      ..blueprint::JwtProvider::test_value()
    };
    let jwt_provider = JwtProvider::new(jwt_options, Arc::new(MockHttpClient));

    let error = jwt_provider
      .validate(&create_jwt_auth_request(JWT_VALID_TOKEN_WITH_KID))
      .await
      .err();

    assert_eq!(error, Some(AuthError::Invalid));
  }
}
