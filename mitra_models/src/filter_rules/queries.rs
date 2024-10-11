use crate::database::{DatabaseClient, DatabaseError};

use super::types::{
    FilterAction,
    FilterRule,
};

pub async fn add_filter_rule(
    db_client: &impl DatabaseClient,
    target: &str,
    action: FilterAction,
    is_reversed: bool,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO filter_rule (
            target,
            filter_action,
            is_reversed
        )
        VALUES ($1, $2, $3)
        ON CONFLICT (target, filter_action)
        DO UPDATE SET is_reversed = $3
        ",
        &[&target, &action, &is_reversed],
    ).await?;
    Ok(())
}

pub async fn remove_filter_rule(
    db_client: &impl DatabaseClient,
    target: &str,
    action: FilterAction,
) -> Result<(), DatabaseError> {
    let deleted_count = db_client.execute(
        "
        DELETE FROM filter_rule
        WHERE target = $1 AND filter_action = $2
        ",
        &[&target, &action],
    ).await?;
    if deleted_count == 0 {
        return Err(DatabaseError::NotFound("filter rule"));
    };
    Ok(())
}

pub async fn get_filter_rules(
    db_client: &impl DatabaseClient,
) -> Result<Vec<FilterRule>, DatabaseError> {
    // Ordering: from less to more specific rules
    let rows = db_client.query(
        "
        SELECT filter_rule
        FROM filter_rule
        ORDER BY
            length(target) ASC,
            reverse(target) ASC,
            filter_action ASC
        ",
        &[],
    ).await?;
    let rules = rows.iter()
        .map(|row| row.try_get("filter_rule"))
        .collect::<Result<_, _>>()?;
    Ok(rules)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_add_remove_filter_rule() {
        let db_client = &create_test_database().await;
        let hostname = "bad.example";
        let action = FilterAction::Reject;

        add_filter_rule(
            db_client,
            hostname,
            action,
            false,
        ).await.unwrap();
        let policies = get_filter_rules(db_client).await.unwrap();
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].target, hostname);
        assert_eq!(policies[0].is_reversed, false);

        add_filter_rule(
            db_client,
            hostname,
            action,
            true,
        ).await.unwrap();
        let rules = get_filter_rules(db_client).await.unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].is_reversed, true);

        remove_filter_rule(
            db_client,
            hostname,
            action,
        ).await.unwrap();
        let rules = get_filter_rules(db_client).await.unwrap();
        assert_eq!(rules.len(), 0);
    }
}
