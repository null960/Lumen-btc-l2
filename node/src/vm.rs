use solana_sdk::pubkey::Pubkey;
use solana_sdk::account::Account;
use std::collections::HashMap;
use std::error::Error;
use wasmi::{Engine, Module, Store, Linker, Value};

pub struct ExecutionResult {
    pub new_accounts: HashMap<Pubkey, Account>,
    pub logs: Vec<String>,
}

pub struct LumenVM;

impl LumenVM {
    pub fn execute(
        program_id: &Pubkey,
        accounts: &mut HashMap<Pubkey, Account>,
        bytecode: &[u8],
        gas_limit: u64,
    ) -> Result<ExecutionResult, Box<dyn Error>> {
        let mut logs = Vec::new();
        logs.push("🚀 Lumen WASM Runtime Booting...".to_string());
        logs.push(format!("📜 Target Program: {}", program_id));

        if bytecode.is_empty() {
            return Err("Bytecode is empty".into());
        }

        // Init engine
        let engine = Engine::default();
        
        // Compile module
        let module = match Module::new(&engine, bytecode) {
            Ok(m) => m,
            Err(e) => {
                logs.push(format!("❌ WASM compilation failed: {}", e));
                return Err(e.into());
            }
        };

        // Setup store and linker
        let mut store = Store::new(&engine, ());
        let linker = <Linker<()>>::new(&engine);

        // Instantiate
        let instance = linker.instantiate(&mut store, &module)?.start(&mut store)?;

        // Get process func
        let process = instance
            .get_export(&store, "process")
            .and_then(|e| e.into_func())
            .ok_or("Contract missing 'process' export")?;

        // Prepare state
        let user_account = accounts.values_mut().next().unwrap();
        let current_balance = user_account.lamports as i64;
        
        logs.push(format!("▶️ Calling 'process' with balance: {}", current_balance));

        // Run WASM func
        let mut results = [Value::I64(0)];
        match process.call(&mut store, &[Value::I64(current_balance)], &mut results) {
            Ok(_) => {
                if let Value::I64(new_balance) = results[0] {
                    user_account.lamports = new_balance as u64;
                    logs.push(format!("✅ Contract returned new balance: {}", new_balance));
                }
            },
            Err(e) => {
                logs.push(format!("❌ Contract execution trapped: {}", e));
                return Err(e.into());
            }
        }

        logs.push("🏁 WASM Execution finished".to_string());

        Ok(ExecutionResult {
            new_accounts: accounts.clone(),
            logs,
        })
    }
}