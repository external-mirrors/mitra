// http://nodeinfo.diaspora.software/protocol.html
use actix_web::{get, web, HttpResponse};
use apx_sdk::jrd::{JsonResourceDescriptor, Link};

use mitra_config::Config;
use mitra_models::database::{get_database_client, DatabaseConnectionPool};

use crate::errors::HttpError;

use super::helpers::{get_instance_staff, get_usage};
use super::types::{Metadata, NodeInfo20, NodeInfo21};

const NODEINFO_2_0_RELATION_TYPE: &str = "http://nodeinfo.diaspora.software/ns/schema/2.0";
const NODEINFO_2_1_RELATION_TYPE: &str = "http://nodeinfo.diaspora.software/ns/schema/2.1";

#[get("/.well-known/nodeinfo")]
pub async fn get_nodeinfo_jrd(
    config: web::Data<Config>,
) -> Result<HttpResponse, HttpError> {
    let nodeinfo_2_0_url = format!("{}/nodeinfo/2.0", config.instance_url());
    let nodeinfo_2_0_link = Link::new(NODEINFO_2_0_RELATION_TYPE)
        .with_href(&nodeinfo_2_0_url);
    let nodeinfo_2_1_url = format!("{}/nodeinfo/2.1", config.instance_url());
    let nodeinfo_2_1_link = Link::new(NODEINFO_2_1_RELATION_TYPE)
        .with_href(&nodeinfo_2_1_url);
    let jrd = JsonResourceDescriptor {
        subject: config.instance_url(),
        links: vec![nodeinfo_2_0_link, nodeinfo_2_1_link],
    };
    let response = HttpResponse::Ok().json(jrd);
    Ok(response)
}

#[get("/nodeinfo/2.0")]
pub async fn get_nodeinfo_2_0(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let usage = get_usage(db_client).await?;
    let instance_staff = get_instance_staff(&config, db_client).await?;
    let metadata = Metadata::new(&config, instance_staff);
    let nodeinfo = NodeInfo20::new(&config, usage, metadata);
    let response = HttpResponse::Ok().json(nodeinfo);
    Ok(response)
}

#[get("/nodeinfo/2.1")]
pub async fn get_nodeinfo_2_1(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let usage = get_usage(db_client).await?;
    let instance_staff = get_instance_staff(&config, db_client).await?;
    let metadata = Metadata::new(&config, instance_staff);
    let nodeinfo = NodeInfo21::new(&config, usage, metadata);
    let response = HttpResponse::Ok().json(nodeinfo);
    Ok(response)
}
