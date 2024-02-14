use mitra_config::DefaultRole;
use mitra_models::users::types::Role;
use mitra_validators::errors::ValidationError;

pub const ALLOWED_ROLES: [&str; 3] = ["admin", "user", "read_only_user"];

pub fn role_from_str(role_str: &str) -> Result<Role, ValidationError> {
    let role = match role_str {
        "user" => Role::NormalUser,
        "admin" => Role::Admin,
        "read_only_user" => Role::ReadOnlyUser,
        _ => return Err(ValidationError("unknown role")),
    };
    Ok(role)
}

pub fn role_to_str(role: &Role) -> &'static str {
    match role {
        Role::Guest => "guest",
        Role::NormalUser => "user",
        Role::Admin => "admin",
        Role::ReadOnlyUser => "read_only_user",
    }
}

pub fn from_default_role(value: &DefaultRole) -> Role {
    match value {
        DefaultRole::NormalUser => Role::NormalUser,
        DefaultRole::ReadOnlyUser => Role::ReadOnlyUser,
    }
}
