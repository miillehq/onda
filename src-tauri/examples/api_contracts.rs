#[cfg(feature = "wire-schema")]
#[path = "support/request_schema.rs"]
mod request_schema;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let check = std::env::args().any(|value| value == "--check");
    onda_app::api::export_contracts(check)?;
    #[cfg(feature = "wire-schema")]
    request_schema::export(check)?;
    Ok(())
}
