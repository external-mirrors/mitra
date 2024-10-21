use regex::Regex;

use crate::errors::ValidationError;

const TARGET_RE: &str = r"^[a-z0-9\.\*\?-]+$";

pub fn validate_rule_target(target: &str) -> Result<(), ValidationError> {
    let target_re = Regex::new(TARGET_RE)
        .expect("regexp should be valid");
    if !target_re.is_match(target) {
        return Err(ValidationError("invalid rule target"));
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_rule_target() {
        let target = "social.example";
        assert!(validate_rule_target(target).is_ok());
        let target = "*.social.example";
        assert!(validate_rule_target(target).is_ok());
        let target = "*";
        assert!(validate_rule_target(target).is_ok());
        let target = "xn--rksmrgs-5wao1o.josefsson.org";
        assert!(validate_rule_target(target).is_ok());
        let target = "räksmörgås.josefsson.org";
        assert!(validate_rule_target(target).is_err());
    }
}
