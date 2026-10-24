fn main() -> Result<(), Box<dyn std::error::Error>> {
    eagle_app::api::export_contracts(std::env::args().any(|value| value == "--check"))
}
