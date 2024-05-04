use crate::blueprint::*;
use crate::config;
use crate::config::Field;
use crate::lambda::Expression;
use crate::lambda::Expression::Jq;
use crate::try_fold::TryFold;
use crate::valid::{Valid, Validator};

/*fn validate_data_with_schema(
    config: &config::Config,
    field: &config::Field,
    gql_value: ConstValue,
) -> Valid<(), String> {
    match to_json_schema_for_field(field, config)
        .validate(&gql_value)
        .to_result()
    {
        Ok(_) => Valid::succeed(()),
        Err(err) => Valid::from_validation_err(err.transform(&(|a| a.to_owned()))),
    }
}*/

pub struct CompileExpr<'a> {
    pub query: &'a str,
}

pub fn compile_expr(inputs: CompileExpr) -> Valid<Expression, String> {
    let mut defs = jaq_interpret::ParseCtx::new(vec![]);
    defs.insert_natives(jaq_core::core());
    defs.insert_defs(jaq_std::std());

    let filter = inputs.query;
    let (filter, errs) = jaq_parse::parse(filter, jaq_parse::main());
    let errs = errs
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<String>>()
        .join("\n");

    if !errs.is_empty() {
        return Valid::fail(errs);
    }

    Valid::from_option(filter, errs)
        .map(|v| defs.compile(v))
        .map(Jq)

    /* let config_module = inputs.config_module;
    let field = inputs.field;
    let value = inputs.value;
    let validate = inputs.validate;

    Valid::from(
        DynamicValue::try_from(&value.clone()).map_err(|e| ValidationError::new(e.to_string())),
    )
    .and_then(|value| {
        if !value.is_const() {
            // TODO: Add validation for const with Mustache here
            Valid::succeed(Dynamic(value.to_owned()))
        } else {
            let data = &value;
            match data.try_into() {
                Ok(gql) => {
                    let validation = if validate {
                        validate_data_with_schema(config_module, field, gql)
                    } else {
                        Valid::succeed(())
                    };
                    validation.map(|_| Dynamic(value.to_owned()))
                }
                Err(e) => Valid::fail(format!("invalid JSON: {}", e)),
            }
        }
    })*/
}

pub fn update_const_field<'a>(
) -> TryFold<'a, (&'a ConfigModule, &'a Field, &'a config::Type, &'a str), FieldDefinition, String>
{
    TryFold::<(&ConfigModule, &Field, &config::Type, &str), FieldDefinition, String>::new(
        |(_config_module, field, _, _), b_field| {
            let Some(const_field) = &field.const_field else {
                return Valid::succeed(b_field);
            };

            compile_expr(CompileExpr { query: &const_field.query })
                .map(|resolver| b_field.resolver(Some(resolver)))
        },
    )
}
