use console_web::println;
use indexeddb::IndexedDb;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

async fn main(version: u32) {
    let db = IndexedDb::open("test", version, move |_old_version, db| {
        if version >= 1 {
            let _store = db.create_object_store("contact", "id", true).unwrap();
            // store
            //     .create_index("idx_given_name", "given_name", false)
            //     .unwrap();
            // store
            //     .create_index("idx_family_name", "family_name", false)
            //     .unwrap();
        }
    })
    .await;

    match db {
        Ok(ref db) => println!("Success: {:?}", db),
        Err(ref e) => println!("Error: {:?}", e),
    }
}

#[wasm_bindgen(start)]
pub fn run() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    let version = 1;

    spawn_local(main(version));
}
