use erd_model::FOO;
use std::error::Error;

pub fn main() -> Result<(), Box<dyn Error>> {
    println!("FOO: {:?}", FOO);
    Ok(())
}
