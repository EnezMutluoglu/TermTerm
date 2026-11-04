//! Read-only ownership proof using copied journals; no cleanup API or DLL loading.
#[cfg(windows)]
fn main()->anyhow::Result<()> {
    use std::io::Read;
    let mut journals=vec![];
    for path in std::env::args().skip(1) {
        let mut data=vec![];std::fs::File::open(path)?.take(2_097_153).read_to_end(&mut data)?;
        anyhow::ensure!(data.len()<=2_097_152,"journal_too_large");journals.push(serde_json::from_slice(&data)?);
    }
    anyhow::ensure!(!journals.is_empty() && journals.len()<=3,"one_to_three_journal_copies_required");
    let candidates=termterm_vpn_windows_broker::windows::orphan::preflight(&journals)?;
    println!("{}",serde_json::json!({"readOnly":true,"verified":candidates.iter().map(|v|v.summary()).collect::<Vec<_>>()}));Ok(())
}
#[cfg(not(windows))]
fn main(){eprintln!("Windows owned phantom preflight requires Windows");}
