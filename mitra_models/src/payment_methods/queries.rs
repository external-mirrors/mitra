use apx_core::caip2::ChainId;
use uuid::Uuid;

use crate::{
    database::{DatabaseClient, DatabaseError},
    invoices::types::DbChainId,
};

use super::types::{
    PaymentMethod,
    PaymentMethodData,
    PaymentType,
};

pub async fn create_payment_method(
    db_client: &mut impl DatabaseClient,
    method_data: PaymentMethodData,
) -> Result<PaymentMethod, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let row = transaction.query_one(
        "
        INSERT INTO payment_method (
            owner_id,
            payment_type,
            chain_id,
            payout_address,
            view_key
        )
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (owner_id, chain_id)
        DO UPDATE SET
            payment_type = $2,
            payout_address = $4,
            view_key = $5,
            updated_at = CURRENT_TIMESTAMP
        RETURNING payment_method
        ",
        &[
            &method_data.owner_id,
            &method_data.payment_type,
            &DbChainId::new(&method_data.chain_id),
            &method_data.payout_address,
            &method_data.view_key,
        ],
    ).await?;
    let payment_method: PaymentMethod = row.try_get("payment_method")?;
    payment_method.check_consistency()?;
    transaction.commit().await?;
    Ok(payment_method)
}

pub async fn get_payment_method_by_chain_id(
    db_client: &impl DatabaseClient,
    owner_id: Uuid,
    chain_id: &ChainId,
) -> Result<Option<PaymentMethod>, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT payment_method
        FROM payment_method
        WHERE owner_id = $1 AND chain_id = $2
        ",
        &[
            &owner_id,
            &DbChainId::new(chain_id),
        ],
    ).await?;
    let maybe_payment_method = match maybe_row {
        Some(row) => {
            let payment_method: PaymentMethod = row.try_get("payment_method")?;
            payment_method.check_consistency()?;
            Some(payment_method)
        },
        None => None,
    };
    Ok(maybe_payment_method)
}

pub async fn get_payment_methods(
    db_client: &impl DatabaseClient,
    payment_type: PaymentType,
    chain_id: &ChainId,
) -> Result<Vec<PaymentMethod>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT payment_method
        FROM payment_method
        WHERE payment_type = $1 AND chain_id = $2
        ",
        &[
            &payment_type,
            &DbChainId::new(chain_id),
        ],
    ).await?;
    let payment_methods = rows.into_iter()
        .map(|row| row.try_get("payment_method"))
        .collect::<Result<_, _>>()?;
    Ok(payment_methods)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        payment_methods::types::PaymentType,
        users::test_utils::create_test_user,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_payment_method() {
        let db_client = &mut create_test_database().await;
        let user = create_test_user(db_client, "test").await;
        let method_data = PaymentMethodData {
            owner_id: user.id,
            payment_type: PaymentType::Monero,
            chain_id: ChainId::monero_mainnet(),
            payout_address: "abcd".to_owned(),
            view_key: None,
        };
        let method = create_payment_method(
            db_client,
            method_data,
        ).await.unwrap();
        assert_eq!(method.owner_id, user.id);
        assert_eq!(method.payment_type, PaymentType::Monero);
        assert_eq!(method.chain_id.inner(), &ChainId::monero_mainnet());

        let maybe_method_requested = get_payment_method_by_chain_id(
            db_client,
            user.id,
            &ChainId::monero_mainnet(),
        ).await.unwrap();
        assert_eq!(maybe_method_requested.unwrap().id, method.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_update_payment_method() {
        let db_client = &mut create_test_database().await;
        let user = create_test_user(db_client, "test").await;
        let method_data = PaymentMethodData {
            owner_id: user.id,
            payment_type: PaymentType::Monero,
            chain_id: ChainId::monero_mainnet(),
            payout_address: "abcd".to_owned(),
            view_key: None,
        };
        let method = create_payment_method(
            db_client,
            method_data,
        ).await.unwrap();
        let method_data = PaymentMethodData {
            owner_id: user.id,
            payment_type: PaymentType::Monero,
            chain_id: ChainId::monero_mainnet(),
            payout_address: "1234".to_owned(),
            view_key: None,
        };
        let method_updated = create_payment_method(
            db_client,
            method_data,
        ).await.unwrap();
        assert_eq!(method_updated.id, method.id);
        assert_eq!(method_updated.payout_address, "1234");
        assert!(method_updated.updated_at > method.updated_at);
    }
}
