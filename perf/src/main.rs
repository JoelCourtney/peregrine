use clap::Parser;
use peregrine::{op, plan::{Duration, Resource, Time}, run};

#[derive(Parser)]
struct Args {
    num_ops: usize,
}

fn main() {
    let args = Args::parse();

    println!("Running {} operations", args.num_ops);
    
    let mut time = Time::from_tai_seconds(0.0);
    
    let x = Resource::new(1);
    
    for _ in 0..args.num_ops {
        time += Duration::from_seconds(1.0);
        x.mutate_at(time, |x| op! { i!(x) + 1 });
        
        time += Duration::from_seconds(1.0);
        // y.mutate_at(time, |y| op! { i!(y) + 1 });
    }
    
    println!("x: {}", run(x.get_at_inc(time)));
}
