fn main() -> Result<(), Box<dyn std::error::Error>> {
    onda_app::api::export_contracts(std::env::args().any(|value| value == "--check"))
}
