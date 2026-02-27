#![no_std]

// FIXED: Modern Rust requires explicit unsafe for no_mangle
#[unsafe(no_mangle)]
pub extern "C" fn process(current_balance: i64) -> i64 {
    // DeFi Magic: Calculate a 5% yield reward and add it
    let reward = current_balance / 20; 
    current_balance + reward
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}