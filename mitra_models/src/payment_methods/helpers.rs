use apx_core::caip2::ChainId;
use uuid::Uuid;

use crate::{
    database::{DatabaseClient, DatabaseError},
};

use super::{
    queries::get_payment_method_by_chain_id,
    types::{PaymentMethod, PaymentType},
};

pub async fn get_payment_method_by_type_and_chain_id(
    db_client: &impl DatabaseClient,
    owner_id: Uuid,
    payment_type: PaymentType,
    chain_id: &ChainId,
) -> Result<Option<PaymentMethod>, DatabaseError> {
    let maybe_payment_method = get_payment_method_by_chain_id(
        db_client,
        owner_id,
        chain_id,
    )
        .await?
        .filter(|method| method.payment_type == payment_type);
    Ok(maybe_payment_method)
}
