use tigerbeetle as tb;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    futures::executor::block_on(main_async())
}

async fn main_async() -> Result<(), Box<dyn std::error::Error>> {
    println!("hello world");
}
