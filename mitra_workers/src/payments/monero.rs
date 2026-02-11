use chrono::Utc;

use mitra_activitypub::builders::{
    add_person::prepare_add_subscriber,
    update_agreement::prepare_update_agreement,
};
use mitra_adapters::{
    payments::{
        monero::{
            invoice_payment_address,
            payment_method_payout_address,
            payment_method_view_key,
            PaymentError,
            MONERO_INVOICE_TIMEOUT,
        },
        subscriptions::create_or_update_local_subscription,
    },
};
use mitra_config::{
    Instance,
    MoneroConfig,
    MoneroLightConfig,
};
use mitra_models::{
    database::{
        db_client_await,
        get_database_client,
        DatabaseClient,
        DatabaseConnectionPool,
        DatabaseError,
    },
    invoices::{
        helpers::{
            local_invoice_forwarded,
            local_invoice_reopened,
            local_monero_light_invoice_paid,
            local_monero_light_invoice_reopened,
        },
        queries::{
            create_local_invoice,
            get_local_invoice_by_address,
            get_local_invoices_by_status,
            set_invoice_status,
        },
        types::{Invoice, InvoiceStatus},
    },
    notifications::helpers::create_subscriber_payment_notification,
    payment_methods::{
        helpers::get_payment_method_by_type_and_chain_id,
        queries::get_payment_methods,
        types::PaymentType,
    },
    profiles::queries::get_profile_by_id,
    users::queries::{
        get_anonymous_system_account_id,
        get_user_by_id,
    },
};
use mitra_services::monero::{
    light_wallet::{
        LightWalletClient,
    },
    wallet::{
        build_wallet_client,
        get_active_addresses,
        get_incoming_transfers,
        get_latest_incoming_transfer,
        get_subaddress_balance,
        get_subaddress_by_index,
        get_subaddress_index,
        get_transaction_by_id,
        open_monero_wallet,
        send_monero,
        MoneroError,
        TransferCategory,
        WalletClient,
    },
    utils::{
        get_payment_id,
        parse_monero_address,
        LOCK_DURATION,
    },
};

const MONERO_SEND_TIMEOUT: u64 = 120;

async fn send_invoice_status_update(
    instance: &Instance,
    db_client: &impl DatabaseClient,
    invoice: &Invoice,
) -> Result<(), DatabaseError> {
    let sender = get_profile_by_id(db_client, invoice.sender_id).await?;
    let recipient = get_user_by_id(db_client, invoice.recipient_id).await?;
    let maybe_payment_info =
        recipient.profile.monero_subscription(invoice.chain_id.inner());
    let Some(payment_info) = maybe_payment_info else {
        log::error!(
            "subscription is not configured for user {}",
            recipient,
        );
        return Ok(());
    };
    if let Some(ref remote_payer) = sender.actor_json {
        prepare_update_agreement(
            instance,
            &recipient,
            &payment_info,
            invoice,
            remote_payer,
        )?.save_and_enqueue(db_client).await?;
    };
    Ok(())
}

async fn check_open_invoices(
    instance: &Instance,
    config: &MoneroConfig,
    db_pool: &DatabaseConnectionPool,
    wallet_client: &WalletClient,
) -> Result<(), PaymentError> {
    let db_client = &mut **get_database_client(db_pool).await?;
    // Invoices waiting for payment
    let mut address_waitlist = vec![];
    let open_invoices = get_local_invoices_by_status(
        db_client,
        PaymentType::Monero,
        &config.chain_id,
        InvoiceStatus::Open,
    ).await?;
    for invoice in open_invoices {
        let expires_at = invoice.expires_at(MONERO_INVOICE_TIMEOUT);
        if expires_at <= Utc::now() {
            log::info!("invoice {}: timed out", invoice.id);
            set_invoice_status(
                db_client,
                invoice.id,
                InvoiceStatus::Timeout,
            ).await?;
            continue;
        };
        let payment_address = invoice_payment_address(&invoice)?;
        let address_index = get_subaddress_index(
            wallet_client,
            config.account_index,
            &payment_address,
        ).await?;
        address_waitlist.push(address_index.minor);
    };

    if !address_waitlist.is_empty() {
        log::info!("{} invoices are waiting for payment", address_waitlist.len());
        let transfers = get_incoming_transfers(
            wallet_client,
            config.account_index,
            address_waitlist,
        ).await?;
        for transfer in transfers {
            let subaddress = get_subaddress_by_index(
                wallet_client,
                &transfer.subaddr_index,
            ).await?;
            let invoice = get_local_invoice_by_address(
                db_client,
                PaymentType::Monero,
                &config.chain_id,
                &subaddress.to_string(),
            ).await?;
            log::info!(
                "received payment for invoice {}: {}",
                invoice.id,
                transfer.amount,
            );
            if invoice.invoice_status == InvoiceStatus::Open {
                let invoice = set_invoice_status(
                    db_client,
                    invoice.id,
                    InvoiceStatus::Paid,
                ).await?;
                send_invoice_status_update(instance, db_client, &invoice).await?;
            } else {
                log::warn!("invoice has already been paid");
            };
        };
    };
    Ok(())
}

async fn check_paid_invoices(
    config: &MoneroConfig,
    db_pool: &DatabaseConnectionPool,
    wallet_client: &WalletClient,
) -> Result<(), PaymentError> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let wallet_client_delay_tolerant =
        build_wallet_client(config, MONERO_SEND_TIMEOUT)?;
    // Invoices waiting to be forwarded
    let paid_invoices = get_local_invoices_by_status(
        db_client,
        PaymentType::Monero,
        &config.chain_id,
        InvoiceStatus::Paid,
    ).await?;
    for invoice in paid_invoices {
        let payment_address = invoice_payment_address(&invoice)?;
        let address_index = match get_subaddress_index(
            wallet_client,
            config.account_index,
            &payment_address,
        ).await {
            Ok(address_index) => address_index,
            Err(MoneroError::UnexpectedAccount) => {
                // Re-opened after configuration change?
                log::error!("invoice {}: unexpected account index", invoice.id);
                continue;
            },
            Err(other_error) => return Err(other_error.into()),
        };
        let balance_data = get_subaddress_balance(
            wallet_client,
            &address_index,
        ).await?;
        let latest_transfer = match get_latest_incoming_transfer(
            wallet_client,
            &address_index,
        ).await? {
            Some(transfer) => transfer,
            None => {
                log::error!("invoice {}: incoming transfer doesn't exist", invoice.id);
                continue;
            },
        };
        let confirmations = latest_transfer.confirmations.unwrap_or(0);
        if balance_data.balance != balance_data.unlocked_balance ||
            balance_data.balance.as_pico() == 0
        {
            // Don't forward payment until all outputs are unlocked
            log::info!(
                "invoice {}: waiting for unlock ({}/{})",
                invoice.id,
                // Pending transactions, unexpected locks
                if confirmations >= LOCK_DURATION { 0 } else { confirmations },
                LOCK_DURATION,
            );
            continue;
        };
        if confirmations < config.tx_required_confirmations {
            // Wait for more confirmations
            log::info!(
                "invoice {}: waiting for payment confirmation ({}/{})",
                invoice.id,
                confirmations,
                config.tx_required_confirmations,
            );
            continue;
        };
        log::info!(
            "invoice {}: forwarding {}",
            invoice.id,
            balance_data.unlocked_balance,
        );
        let recipient = get_user_by_id(db_client, invoice.recipient_id).await?;
        let maybe_payment_method = get_payment_method_by_type_and_chain_id(
            db_client,
            recipient.id,
            PaymentType::Monero,
            invoice.chain_id.inner(),
        ).await?;
        let payment_method = if let Some(payment_method) = maybe_payment_method {
            payment_method
        } else {
            log::error!(
                "payment method is not available to user {}",
                recipient,
            );
            continue;
        };
        // Send all available balance to payout address
        let (payout_tx_id, _) = match send_monero(
            &wallet_client_delay_tolerant,
            address_index.major,
            address_index.minor,
            &payment_method.payout_address,
        ).await {
            Ok(payout_info) => payout_info,
            Err(error @ MoneroError::Dust) => {
                log::warn!("invoice {}: {}", invoice.id, error);
                set_invoice_status(
                    db_client,
                    invoice.id,
                    InvoiceStatus::Underpaid,
                ).await?;
                continue;
            },
            Err(other_error) => {
                log::error!(
                    "invoice {}: forwarding failed ({})",
                    invoice.id,
                    other_error,
                );
                return Err(other_error.into());
            },
        };

        local_invoice_forwarded(
            db_client,
            invoice.id,
            &payout_tx_id,
        ).await?;
        log::info!("forwarded payment for invoice {}", invoice.id);
    };
    Ok(())
}

async fn create_or_update_monero_subscription(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    invoice: Invoice,
    transfer_amount: u64, // piconero
) -> Result<(), PaymentError> {
    assert_eq!(invoice.invoice_status, InvoiceStatus::Completed);
    let sender = get_profile_by_id(db_client, invoice.sender_id).await?;
    let recipient = get_user_by_id(db_client, invoice.recipient_id).await?;
    let maybe_subscription_info = recipient.profile.monero_subscription(invoice.chain_id.inner());
    let subscription_info = if let Some(subscription_info) = maybe_subscription_info {
        subscription_info
    } else {
        log::warn!(
            "subscription is not configured for user {}",
            recipient,
        );
        return Ok(());
    };
    let duration_secs = (transfer_amount / subscription_info.price)
        .try_into()
        .map_err(|_| MoneroError::OtherError("amount is too big"))?;
    let subscription = create_or_update_local_subscription(
        db_client,
        &sender,
        &recipient,
        duration_secs,
    ).await?;
    create_subscriber_payment_notification(
        db_client,
        sender.id,
        recipient.id,
    ).await?;
    if let Some(ref remote_sender) = sender.actor_json {
        prepare_add_subscriber(
            instance,
            remote_sender,
            &recipient,
            subscription.expires_at,
            Some(invoice.id),
        ).save_and_enqueue(db_client).await?;
    };
    Ok(())
}

async fn check_forwarded_invoices(
    config: &MoneroConfig,
    db_pool: &DatabaseConnectionPool,
    instance: &Instance,
    wallet_client: &WalletClient,
) -> Result<(), PaymentError> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let forwarded_invoices = get_local_invoices_by_status(
        db_client,
        PaymentType::Monero,
        &config.chain_id,
        InvoiceStatus::Forwarded,
    ).await?;
    for invoice in forwarded_invoices {
        let payout_tx_id = if let Some(ref payout_tx_id) = invoice.payout_tx_id {
            payout_tx_id
        } else {
            // Legacy invoices don't have payout_tx_id.
            // Assume payment was fully processed and subscription was updated
            log::warn!("invoice {}: no payout transaction ID", invoice.id);
            set_invoice_status(
                db_client,
                invoice.id,
                InvoiceStatus::Completed,
            ).await?;
            continue;
        };
        let transfer = match get_transaction_by_id(
            wallet_client,
            config.account_index,
            payout_tx_id,
        ).await {
            Ok(maybe_transfer) => {
                if let Some(transfer) = maybe_transfer {
                    transfer
                } else {
                    // Re-opened after configuration change?
                    log::error!(
                        "invoice {}: payout transaction doesn't exist",
                        invoice.id,
                    );
                    continue;
                }
            },
            Err(MoneroError::TooManyRequests) => {
                // Retry later
                log::warn!("invoice {}: wallet is busy", invoice.id);
                continue;
            },
            Err(other_error) => return Err(other_error.into()),
        };
        match transfer.transfer_type {
            TransferCategory::Pending | TransferCategory::Out => (),
            TransferCategory::Failed => {
                log::error!("invoice {}: payout transaction failed", invoice.id);
                set_invoice_status(
                    db_client,
                    invoice.id,
                    InvoiceStatus::Failed,
                ).await?;
                continue;
            },
            _ => {
                log::error!(
                    "invoice {}: unexpected payout transfer type ({:?})",
                    invoice.id,
                    transfer.transfer_type,
                );
                continue;
            },
        };
        let confirmations = transfer.confirmations.unwrap_or(0);
        if confirmations < config.tx_required_confirmations {
            // Wait for more confirmations
            log::info!(
                "invoice {}: waiting for payout confirmation ({}/{})",
                invoice.id,
                confirmations,
                config.tx_required_confirmations,
            );
            continue;
        };
        log::info!(
            "payout transaction confirmed for invoice {} ({})",
            invoice.id,
            transfer.amount,
        );
        let invoice = set_invoice_status(
            db_client,
            invoice.id,
            InvoiceStatus::Completed,
        ).await?;

        // Optional: update subscription
        create_or_update_monero_subscription(
            db_client,
            instance,
            invoice,
            transfer.amount.as_pico(),
        ).await?;
    };
    Ok(())
}

pub async fn check_monero_invoices(
    instance: &Instance,
    config: &MoneroConfig,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), PaymentError> {
    let wallet_client = open_monero_wallet(config).await?;
    check_open_invoices(instance, config, db_pool, &wallet_client).await?;
    check_paid_invoices(config, db_pool, &wallet_client).await?;
    check_forwarded_invoices(config, db_pool, instance, &wallet_client).await?;
    Ok(())
}

pub async fn check_closed_monero_invoices(
    config: &MoneroConfig,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), PaymentError> {
    let wallet_client = open_monero_wallet(config).await?;
    let addresses = get_active_addresses(
        &wallet_client,
        config.account_index,
    ).await?;
    let db_client = &mut **get_database_client(db_pool).await?;
    for (address, _) in addresses {
        let invoice = match get_local_invoice_by_address(
            db_client,
            PaymentType::Monero,
            &config.chain_id,
            &address.to_string(),
        ).await {
            Ok(invoice) => invoice,
            Err(DatabaseError::NotFound(_)) => {
                log::error!(
                    "invoice with address {} doesn't exist",
                    address,
                );
                continue;
            },
            Err(other_error) => return Err(other_error.into()),
        };
        if !invoice.invoice_status.is_final() {
            // Open, Paid, etc
            continue;
        };
        log::info!(
            "invoice {} ({:?}): new payment detected",
            invoice.id,
            invoice.invoice_status,
        );
        local_invoice_reopened(db_client, invoice.id).await?;
    };
    Ok(())
}

async fn check_monero_light_open_invoices(
    db_pool: &DatabaseConnectionPool,
    config: &MoneroLightConfig,
    instance: &Instance,
) -> Result<(), PaymentError> {
    let db_client = &mut **get_database_client(db_pool).await?;
    // Invoices waiting for payment
    let open_invoices = get_local_invoices_by_status(
        db_client,
        PaymentType::MoneroLight,
        &config.chain_id,
        InvoiceStatus::Open,
    ).await?;
    for invoice in open_invoices {
        let expires_at = invoice.expires_at(MONERO_INVOICE_TIMEOUT);
        if expires_at <= Utc::now() {
            log::info!("invoice {}: timed out", invoice.id);
            set_invoice_status(
                db_client,
                invoice.id,
                InvoiceStatus::Timeout,
            ).await?;
            continue;
        };
        let recipient = get_user_by_id(db_client, invoice.recipient_id).await?;
        let maybe_payment_method = get_payment_method_by_type_and_chain_id(
            db_client,
            recipient.id,
            PaymentType::MoneroLight,
            invoice.chain_id.inner(),
        ).await?;
        let payment_method = if let Some(payment_method) = maybe_payment_method {
            payment_method
        } else {
            log::error!(
                "payment method is not available to user {}",
                recipient,
            );
            continue;
        };
        let payment_address = invoice_payment_address(&invoice)?;
        if payment_address == payment_method.payout_address {
            // Ignore invoices for anonymous payments
            continue;
        };
        let payout_address = payment_method_payout_address(&payment_method)?;
        let view_key = payment_method_view_key(&payment_method)?;
        let lw_client = LightWalletClient::new(
            config,
            payout_address,
            view_key,
        );
        let payment_address = parse_monero_address(&payment_address)
            .map_err(|_| DatabaseError::type_error())?;
        let payment_id = get_payment_id(payment_address)
            .ok_or(DatabaseError::type_error())?;
        // Wait for incoming transaction
        let maybe_payout_tx_id = lw_client.get_tx_id_by_payment_id(payment_id).await?;
        let Some(payout_tx_id) = maybe_payout_tx_id else {
            // Transaction not found
            continue;
        };
        log::info!(
            "received payment for invoice {}: {}",
            invoice.id,
            payout_tx_id,
        );
        let invoice = local_monero_light_invoice_paid(
            db_client,
            invoice.id,
            &payout_tx_id,
        ).await?;
        send_invoice_status_update(instance, db_client, &invoice).await?;
    };
    Ok(())
}

async fn check_monero_light_paid_invoices(
    db_pool: &DatabaseConnectionPool,
    config: &MoneroLightConfig,
    instance: &Instance,
) -> Result<(), PaymentError> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let paid_invoices = get_local_invoices_by_status(
        db_client,
        PaymentType::MoneroLight,
        &config.chain_id,
        InvoiceStatus::Paid,
    ).await?;
    for invoice in paid_invoices {
        let Some(ref payout_tx_id) = invoice.payout_tx_id else {
            log::error!("invoice {}: no payout transaction ID", invoice.id);
            continue;
        };
        let recipient = get_user_by_id(db_client, invoice.recipient_id).await?;
        let maybe_payment_method = get_payment_method_by_type_and_chain_id(
            db_client,
            recipient.id,
            PaymentType::MoneroLight,
            invoice.chain_id.inner(),
        ).await?;
        let payment_method = if let Some(payment_method) = maybe_payment_method {
            payment_method
        } else {
            log::error!(
                "payment method is not available to user {}",
                recipient,
            );
            continue;
        };
        let payout_address = payment_method_payout_address(&payment_method)?;
        let view_key = payment_method_view_key(&payment_method)?;
        let lw_client = LightWalletClient::new(
            config,
            payout_address,
            view_key,
        );
        let (tx_amount, confirmations) = lw_client.get_tx_info(payout_tx_id).await?;
        if confirmations < config.tx_required_confirmations {
            // Wait for more confirmations
            log::info!(
                "invoice {}: waiting for payment confirmation ({}/{})",
                invoice.id,
                confirmations,
                config.tx_required_confirmations,
            );
            continue;
        };
        log::info!(
            "payment transaction confirmed for invoice {} ({})",
            invoice.id,
            tx_amount,
        );
        let invoice = set_invoice_status(
            db_client,
            invoice.id,
            InvoiceStatus::Completed,
        ).await?;

        // Optional: update subscription
        create_or_update_monero_subscription(
            db_client,
            instance,
            invoice,
            tx_amount.as_pico(),
        ).await?;
    };
    Ok(())
}

pub async fn check_monero_light_invoices(
    instance: &Instance,
    config: &MoneroLightConfig,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), PaymentError> {
    check_monero_light_open_invoices(db_pool, config, instance).await?;
    check_monero_light_paid_invoices(db_pool, config, instance).await?;
    Ok(())
}

pub async fn check_monero_light_payments(
    config: &MoneroLightConfig,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), PaymentError> {
    let maybe_anonymous_sender_id =
        get_anonymous_system_account_id(db_client_await!(db_pool)).await?;
    let Some(anonymous_sender_id) = maybe_anonymous_sender_id else {
        log::warn!("automated account doesn't exist");
        return Ok(());
    };
    let payment_methods = get_payment_methods(
        db_client_await!(db_pool),
        PaymentType::MoneroLight,
        &config.chain_id,
    ).await?;
    for payment_method in payment_methods {
        let payout_address = payment_method_payout_address(&payment_method)?;
        let view_key = payment_method_view_key(&payment_method)?;
        let maybe_invoice = match get_local_invoice_by_address(
            db_client_await!(db_pool),
            PaymentType::MoneroLight,
            &config.chain_id,
            &payout_address.to_string(),
        ).await {
            Ok(invoice) => {
                if invoice.invoice_status != InvoiceStatus::Completed {
                    // Already processing
                    continue;
                };
                Some(invoice)
            },
            Err(DatabaseError::NotFound(_)) => None,
            Err(other_error) => return Err(other_error.into()),
        };
        let lw_client = LightWalletClient::new(
            config,
            payout_address,
            view_key,
        );
        // Get new transactions
        let since_date = maybe_invoice
            .as_ref()
            .map(|invoice| std::cmp::max(invoice.updated_at, payment_method.updated_at))
            .unwrap_or(payment_method.updated_at);
        let transactions = lw_client
            .get_primary_address_txs(since_date)
            .await?;
        if let Some(tx_id) = transactions.first() {
            let db_client = &mut **get_database_client(db_pool).await?;
            log::info!("detected payment to primary address {tx_id}");
            // No conflict with open invoice monitor because it ignores
            // invoices where payment address matches payout address
            let invoice = if let Some(invoice) = maybe_invoice {
                local_monero_light_invoice_reopened(
                    db_client,
                    invoice.id,
                ).await?
            } else {
                create_local_invoice(
                    db_client,
                    anonymous_sender_id,
                    payment_method.owner_id,
                    payment_method.payment_type,
                    payment_method.chain_id.inner(),
                    &payout_address.to_string(),
                    0, // no expected amount
                ).await?
            };
            local_monero_light_invoice_paid(
                db_client,
                invoice.id,
                tx_id,
            ).await?;
        };
    };
    Ok(())
}
