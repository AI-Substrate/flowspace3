use std::error::Error;
use std::io;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let base_url = std::env::args().nth(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: cargo run -p fs3-testkit --example list_orphans -- <base-url>",
        )
    })?;
    let report = fs3_testkit::FreshDatabase::list_orphans_from(&base_url).await?;
    for name in report.candidates {
        println!("{name}");
    }
    Ok(())
}
