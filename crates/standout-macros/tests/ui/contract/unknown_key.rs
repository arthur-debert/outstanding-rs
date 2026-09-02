use standout_macros::ContractSurface;

#[derive(ContractSurface)]
#[contract(version = 1)]
struct Listing {
    items: Vec<String>,
}

fn main() {}
