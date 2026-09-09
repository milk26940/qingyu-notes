use std::fs;
use std::time::{Duration, Instant};

use rusqlite::{params, Connection};
use uuid::Uuid;

use super::repository::PageRepository;

#[test]
#[ignore = "性能基准需显式运行"]
fn ten_thousand_pages_open_and_search_meet_targets() {
    let root = std::env::temp_dir().join(format!("qingyu-10000-benchmark-{}", Uuid::new_v4()));
    let repo = PageRepository::open(root.join("vault")).unwrap();
    let mut connection = Connection::open(repo.database_path()).unwrap();
    let transaction = connection.transaction().unwrap();
    for index in 0..10_000 {
        let page_id = format!("benchmark-page-{index:05}");
        let block_id = format!("benchmark-block-{index:05}");
        let title = format!("性能页面 {index:05}");
        let text = if index == 9_999 {
            "唯一性能关键字"
        } else {
            "普通测试内容"
        };
        transaction.execute(
            "INSERT INTO pages(id,title,parent_id,kind,created_at,updated_at,trashed_at,sort_order,properties_json,database_id,revision) VALUES(?1,?2,NULL,'page','0','0',NULL,?3,'{}',NULL,0)",
            params![page_id, title, index as i64],
        ).unwrap();
        transaction.execute(
            "INSERT INTO blocks(id,page_id,parent_block_id,position,block_type,text,attrs_json) VALUES(?1,?2,NULL,0,'paragraph',?3,'{}')",
            params![block_id, page_id, text],
        ).unwrap();
        transaction.execute(
            "INSERT INTO blocks_fts(block_id,page_id,text) VALUES(?1,?2,?3)",
            params![block_id, page_id, text],
        ).unwrap();
    }
    transaction.commit().unwrap();
    drop(connection);

    let open_started = Instant::now();
    let bundle = repo.open_bundle("benchmark-page-09999").unwrap();
    let open_elapsed = open_started.elapsed();
    assert_eq!(bundle.page.title, "性能页面 09999");

    let search_started = Instant::now();
    let results = repo.search("性能关键字").unwrap();
    let search_elapsed = search_started.elapsed();
    assert_eq!(results.len(), 1);
    assert!(open_elapsed <= Duration::from_millis(50), "按 ID 打开耗时 {open_elapsed:?}");
    assert!(search_elapsed <= Duration::from_millis(150), "全文搜索耗时 {search_elapsed:?}");
    eprintln!("10000 页基准：打开={open_elapsed:?}，搜索={search_elapsed:?}");

    drop(repo);
    let _ = fs::remove_dir_all(root);
}

#[test]
#[ignore = "真实库基准需设置 QINGYU_BENCH_VAULT"]
fn real_library_page_open_benchmark() {
    let vault = std::env::var_os("QINGYU_BENCH_VAULT")
        .map(std::path::PathBuf::from)
        .expect("缺少 QINGYU_BENCH_VAULT");
    let repo = PageRepository::open(vault).unwrap();
    let pages = repo.list(false).unwrap();
    assert!(!pages.is_empty());
    let mut maximum = Duration::ZERO;
    let mut total = Duration::ZERO;
    for page in &pages {
        let started = Instant::now();
        repo.open_bundle(&page.id).unwrap();
        let elapsed = started.elapsed();
        maximum = maximum.max(elapsed);
        total += elapsed;
    }
    let query = pages[0].title.chars().take(3).collect::<String>();
    let search_started = Instant::now();
    let _ = repo.search(&query).unwrap();
    let search_elapsed = search_started.elapsed();
    let average = total / pages.len() as u32;
    assert!(maximum <= Duration::from_millis(40), "真实库最慢页面打开耗时 {maximum:?}");
    assert!(search_elapsed <= Duration::from_millis(50), "真实库搜索耗时 {search_elapsed:?}");
    eprintln!(
        "真实库基准：页面数={}，平均打开={average:?}，最慢打开={maximum:?}，搜索={search_elapsed:?}",
        pages.len()
    );
}
