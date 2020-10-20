use super::{ISideChain, SideChainBlock, SideChainTx};

use rusqlite::{params, NO_PARAMS};

use rusqlite::Connection as DB;

/// Implementation of ISideChain that uses sqlite to
/// persist between restarts
pub struct PeristentSideChain {
    blocks: Vec<SideChainBlock>,
    db: DB,
}

fn create_tables_if_new(db: &DB) {
    db.execute(
        "CREATE TABLE IF NOT EXISTS blocks (
        block_id INTEGER PRIMARY KEY,
        data BLOB NOT NULL
    )",
        NO_PARAMS,
    )
    .expect("could not create or open DB");
}

fn parse_tuple(tuple: (u32, String)) -> Result<(u32, SideChainBlock), String> {
    let block: SideChainBlock = serde_json::from_str(&tuple.1).map_err(|err| err.to_string())?;

    Ok((tuple.0, block))
}

fn read_rows(db: &DB) -> Result<Vec<SideChainBlock>, String> {
    let mut stmt = db
        .prepare("SELECT block_id, data FROM blocks;")
        .expect("Could not prepare stmt");

    let res: Vec<Result<(u32, String), _>> = stmt
        .query_map(NO_PARAMS, |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|err| err.to_string())?
        .collect();

    let mut block_tuples: Vec<(u32, SideChainBlock)> = Vec::with_capacity(res.len());

    for row_res in res {
        let tuple = parse_tuple(row_res.map_err(|err| err.to_string())?)?;
        block_tuples.push(tuple);
    }

    // The rows in the database are not guaranteed to be ordered:
    block_tuples.sort_unstable_by_key(|(idx, _)| *idx);

    // Sanity check
    if let Some(last) = block_tuples.last() {
        if last.0 as usize + 1 != block_tuples.len() {
            return Err(format!(
                "Unexpected last block index: {} (expected {})",
                last.0,
                block_tuples.len() - 1
            ));
        }
    }

    let blocks = block_tuples.into_iter().map(|(_, block)| block).collect();

    Ok(blocks)
}

impl PeristentSideChain {
    /// Create a instance of PeristentSideChain associated with a database file
    /// with name `file`. The file is created if does not exist. The database tables
    /// are created they don't already exist.
    pub fn open(file: &str) -> Self {
        let db = DB::open(file).expect("Could not open the database");

        create_tables_if_new(&db);

        // Not being able to load the database is going to be a hard error
        let blocks = match read_rows(&db) {
            Ok(blocks) => blocks,
            Err(err) => {
                error!("Could not load blocks from the database: {}", err);
                panic!();
            }
        };

        debug!("Loaded {} blocks from the database", blocks.len());

        PeristentSideChain { db, blocks }
    }
}

/// I have a feeling that we might use this function later (if we can't load the
/// entire database in memory), so want to keep it around at least for now
#[allow(dead_code)]
fn get_block_from_db(db: &DB, block_idx: u32) -> Option<SideChainBlock> {
    // We should probably add a cache layer eventually
    let mut stmt = db
        .prepare("SELECT data FROM blocks WHERE block_id = ?")
        .expect("Could not prepare stmt");

    let val: Result<String, _> = stmt.query_row(params![block_idx], |row| row.get(0));

    match val {
        Ok(val) => match serde_json::from_str::<SideChainBlock>(&val) {
            Ok(block) => Some(block),
            Err(err) => {
                error!("Failed to parse block json: {}", err);
                None
            }
        },
        Err(_) => None,
    }
}

impl ISideChain for PeristentSideChain {
    fn add_block(&mut self, txs: Vec<SideChainTx>) -> Result<(), String> {
        let block_idx = self.blocks.len() as u32;
        let block = SideChainBlock {
            id: block_idx,
            transactions: txs,
        };

        let blob = serde_json::to_string(&block).unwrap();

        // TODO: should not be able to replace the entry!
        match self.db.execute(
            "INSERT OR REPLACE INTO blocks (block_id, data) values (?1, ?2)",
            params![block_idx, blob],
        ) {
            Ok(_) => {
                self.blocks.push(block);
                return Ok(());
            }
            Err(err) => {
                eprintln!("Error inserting into the database: {}", err);
                return Err("TODO".to_owned());
            }
        };
    }

    fn get_block(&self, block_idx: u32) -> Option<&SideChainBlock> {
        self.blocks.get(block_idx as usize)
    }

    fn total_blocks(&self) -> u32 {
        self.blocks.len() as u32
    }
}

#[test]
fn should_read_block_after_reopen() {
    use crate::utils::test_utils;

    test_utils::logging::init();

    let temp_file = test_utils::TempRandomFile::new();

    let mut db = PeristentSideChain::open(temp_file.path());

    let tx = test_utils::create_fake_quote_tx_eth_loki();
    let tx = SideChainTx::from(tx);

    db.add_block(vec![tx.clone()])
        .expect("Error adding a transaction to the database");

    // Close the database
    drop(db);

    let db = PeristentSideChain::open(temp_file.path());

    let total_blocks = db.total_blocks();
    let last_block_idx = total_blocks.checked_sub(1).expect("Unexpected block count");
    let last_block = db
        .get_block(last_block_idx)
        .expect("Could not get last block");

    assert_eq!(tx, last_block.transactions[0]);
}
