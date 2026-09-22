use crate::{
    connections::{connect_ssh, SftpConnection},
    state::Shared,
};
use anyhow::{anyhow, ensure, Result};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub directory: bool,
    pub symlink: bool,
    pub size: u64,
    pub modified: u64,
    pub permissions: Option<u32>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub connection: String,
    pub path: String,
}
#[derive(Clone)]
pub struct Edit {
    pub gate: Arc<tokio::sync::Mutex<()>>,
    pub remote: Endpoint,
    pub local: std::path::PathBuf,
    pub hash: Vec<u8>,
}
async fn remote_hash(state: &Shared, endpoint: &Endpoint) -> Result<Vec<u8>> {
    use sha2::{Digest, Sha256};
    let c = connection(state, &endpoint.connection).await?;
    let m = c.sftp.symlink_metadata(&endpoint.path).await?;
    ensure!(
        !m.is_dir() && !m.is_symlink() && m.size.unwrap_or(0) <= 32 * 1024 * 1024,
        "External editing requires a regular file up to 32 MiB"
    );
    let mut file = c.sftp.open(&endpoint.path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];
    let mut size = 0;
    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        size += count;
        ensure!(size <= 32 * 1024 * 1024, "File grew beyond edit limit");
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().to_vec())
}
pub async fn edit_begin(
    app: &AppHandle,
    state: &Shared,
    endpoint: Endpoint,
) -> Result<serde_json::Value> {
    ensure!(endpoint.connection != "local", "Choose a remote file");
    let hash = remote_hash(state, &endpoint).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let folder = app.path().app_cache_dir()?.join("external-edits").join(&id);
    tokio::fs::create_dir_all(&folder).await?;
    let filename = endpoint
        .path
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty() && !s.contains(['\\', ':']))
        .ok_or_else(|| anyhow!("Unsafe filename"))?;
    let local = folder.join(filename);
    copy(
        app,
        state,
        &id,
        endpoint.clone(),
        Endpoint {
            connection: "local".into(),
            path: local.to_string_lossy().into(),
        },
        false,
    )
    .await?;
    ensure!(
        remote_hash(state, &endpoint).await? == hash,
        "Remote file changed during download; retry"
    );
    let editor = state
        .vault
        .lock()
        .ok()
        .and_then(|v| v.as_ref().and_then(|v| v.records().ok()))
        .and_then(|r| r.into_iter().find(|r| r.kind == "settings"))
        .and_then(|r| r.data["externalEditor"].as_str().map(str::to_string));
    crate::platform::edit(&local, editor.as_deref())?;
    let result = serde_json::json!({"id":id,"path":local,"name":filename});
    state.edits.lock().await.insert(
        id,
        Edit {
            gate: Arc::new(tokio::sync::Mutex::new(())),
            remote: endpoint,
            local,
            hash,
        },
    );
    Ok(result)
}
pub async fn edit_save(
    app: &AppHandle,
    state: &Shared,
    id: &str,
    overwrite: bool,
    close: bool,
) -> Result<()> {
    let edit = state
        .edits
        .lock()
        .await
        .get(id)
        .cloned()
        .ok_or_else(|| anyhow!("Edit session closed"))?;
    let _gate = edit
        .gate
        .try_lock()
        .map_err(|_| anyhow!("This edit operation is already running"))?;
    if !close {
        let current = remote_hash(state, &edit.remote).await?;
        ensure!(overwrite||current==edit.hash,"Remote file changed since opening. Review your local copy, then explicitly overwrite to replace the remote version.");
        copy(
            app,
            state,
            id,
            Endpoint {
                connection: "local".into(),
                path: edit.local.to_string_lossy().into(),
            },
            edit.remote.clone(),
            true,
        )
        .await?;
        let hash = remote_hash(state, &edit.remote).await?;
        if let Some(current) = state.edits.lock().await.get_mut(id) {
            current.hash = hash;
        }
    } else {
        let local = edit.local.clone();
        state.edits.lock().await.remove(id);
        let _ = tokio::fs::remove_file(&local).await;
        if let Some(parent) = local.parent() {
            let _ = tokio::fs::remove_dir(parent).await;
        }
    }
    Ok(())
}
pub async fn connect(app: &AppHandle, state: &Shared, id: &str, host_id: &str) -> Result<String> {
    let ssh = connect_ssh(app, state, id, host_id, None).await?;
    let channel = ssh.handle.channel_open_session().await?;
    channel.request_subsystem(true, "sftp").await?;
    let sftp = russh_sftp::client::SftpSession::new(channel.into_stream()).await?;
    sftp.set_timeout(30);
    let home = sftp.canonicalize(".").await?;
    state
        .sftp
        .lock()
        .await
        .insert(id.into(), Arc::new(SftpConnection { sftp, _ssh: ssh }));
    Ok(home)
}
async fn connection(state: &Shared, id: &str) -> Result<Arc<SftpConnection>> {
    state
        .sftp
        .lock()
        .await
        .get(id)
        .cloned()
        .ok_or_else(|| anyhow!("SFTP connection closed"))
}
pub async fn list(state: &Shared, endpoint: &Endpoint) -> Result<Vec<FileEntry>> {
    let mut entries = vec![];
    if endpoint.connection == "local" {
        let mut dir = tokio::fs::read_dir(&endpoint.path).await?;
        while let Some(e) = dir.next_entry().await? {
            let m = e.metadata().await?;
            entries.push(FileEntry {
                name: e.file_name().to_string_lossy().into(),
                path: e.path().to_string_lossy().into(),
                directory: m.is_dir(),
                symlink: m.is_symlink(),
                size: m.len(),
                modified: m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                permissions: None,
            });
        }
    } else {
        let conn = connection(state, &endpoint.connection).await?;
        for e in conn.sftp.read_dir(&endpoint.path).await? {
            let m = e.metadata();
            entries.push(FileEntry {
                name: e.file_name(),
                path: e.path(),
                directory: m.is_dir(),
                symlink: m.is_symlink(),
                size: m.size.unwrap_or(0),
                modified: m.mtime.unwrap_or(0) as u64,
                permissions: m.permissions,
            });
        }
    }
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}
pub async fn action(
    state: &Shared,
    endpoint: &Endpoint,
    action: &str,
    target: Option<String>,
    permissions: Option<u32>,
) -> Result<()> {
    if endpoint.connection == "local" {
        match action {
            "mkdir" => tokio::fs::create_dir(&endpoint.path).await?,
            "rename" => {
                let target = target.ok_or_else(|| anyhow!("New name required"))?;
                ensure!(!Path::new(&target).exists(), "Destination exists");
                tokio::fs::rename(&endpoint.path, target).await?;
            }
            "remove" => {
                let meta = tokio::fs::symlink_metadata(&endpoint.path).await?;
                if meta.is_dir() {
                    tokio::fs::remove_dir(&endpoint.path).await?;
                } else {
                    tokio::fs::remove_file(&endpoint.path).await?;
                }
            }
            _ => anyhow::bail!("Unsupported local file action"),
        }
    } else {
        let c = connection(state, &endpoint.connection).await?;
        match action {
            "mkdir" => c.sftp.create_dir(&endpoint.path).await?,
            "rename" => {
                c.sftp
                    .rename(
                        &endpoint.path,
                        target.ok_or_else(|| anyhow!("New name required"))?,
                    )
                    .await?
            }
            "chmod" => {
                let mode = permissions.ok_or_else(|| anyhow!("Permissions required"))?;
                ensure!(mode <= 0o7777, "Invalid permissions");
                c.sftp
                    .set_metadata(
                        &endpoint.path,
                        russh_sftp::protocol::FileAttributes {
                            permissions: Some(mode),
                            ..Default::default()
                        },
                    )
                    .await?;
            }
            "remove" => {
                let m = c.sftp.symlink_metadata(&endpoint.path).await?;
                if m.is_dir() {
                    c.sftp.remove_dir(&endpoint.path).await?;
                } else {
                    c.sftp.remove_file(&endpoint.path).await?;
                }
            }
            _ => anyhow::bail!("Unsupported remote file action"),
        }
    }
    Ok(())
}

pub async fn copy(
    app: &AppHandle,
    state: &Shared,
    id: &str,
    source: Endpoint,
    dest: Endpoint,
    overwrite: bool,
) -> Result<u64> {
    let control = crate::transfers::start(app, state, id)?;
    let result = copy_inner(app, state, id, source, dest, overwrite, &control).await;
    crate::transfers::finish(app, state, id, &result);
    result
}
async fn fingerprint(
    endpoint: &Endpoint,
    conn: &Option<Arc<SftpConnection>>,
) -> Result<(bool, bool, u64, u128)> {
    if let Some(c) = conn {
        let m = c.sftp.symlink_metadata(&endpoint.path).await?;
        Ok((
            m.is_dir(),
            m.is_symlink(),
            m.size.unwrap_or(0),
            m.mtime.unwrap_or(0) as u128 * 1_000_000_000,
        ))
    } else {
        let m = tokio::fs::symlink_metadata(&endpoint.path).await?;
        Ok((
            m.is_dir(),
            m.is_symlink(),
            m.len(),
            m.modified()?
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos(),
        ))
    }
}
async fn input_file(
    endpoint: &Endpoint,
    conn: &Option<Arc<SftpConnection>>,
) -> Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
    Ok(if let Some(c) = conn {
        Box::new(c.sftp.open(&endpoint.path).await?)
    } else {
        Box::new(tokio::fs::File::open(&endpoint.path).await?)
    })
}
async fn output_file(
    path: &str,
    conn: &Option<Arc<SftpConnection>>,
    restart: bool,
) -> Result<Box<dyn tokio::io::AsyncWrite + Unpin + Send>> {
    Ok(if let Some(c) = conn {
        use russh_sftp::protocol::OpenFlags;
        Box::new(
            c.sftp
                .open_with_flags(
                    path,
                    OpenFlags::WRITE
                        | OpenFlags::CREATE
                        | if restart {
                            OpenFlags::TRUNCATE
                        } else {
                            OpenFlags::EXCLUDE
                        },
                )
                .await?,
        )
    } else {
        Box::new(
            tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(!restart)
                .truncate(restart)
                .open(path)
                .await?,
        )
    })
}
async fn copy_inner(
    app: &AppHandle,
    state: &Shared,
    id: &str,
    source: Endpoint,
    dest: Endpoint,
    overwrite: bool,
    control: &crate::transfers::Control,
) -> Result<u64> {
    control.checkpoint().await?;
    ensure!(
        source.connection != dest.connection || source.path != dest.path,
        "Source and destination are the same"
    );
    let source_conn = if source.connection == "local" {
        None
    } else {
        Some(connection(state, &source.connection).await?)
    };
    let dest_conn = if dest.connection == "local" {
        None
    } else {
        Some(connection(state, &dest.connection).await?)
    };
    let mut metadata = control.io(fingerprint(&source, &source_conn)).await?;
    ensure!(
        !metadata.1,
        "Symbolic links are not followed during transfer"
    );
    if metadata.0 {
        // Reject recursive copies into the source, including canonical aliases locally.
        if source.connection == dest.connection {
            let (from, to) = if source.connection == "local" {
                let from = tokio::fs::canonicalize(&source.path).await?;
                let parent = Path::new(&dest.path)
                    .parent()
                    .ok_or_else(|| anyhow!("Destination parent required"))?;
                let to = tokio::fs::canonicalize(parent).await?.join(
                    Path::new(&dest.path)
                        .file_name()
                        .ok_or_else(|| anyhow!("Destination name required"))?,
                );
                (from, to)
            } else {
                (
                    std::path::PathBuf::from(&source.path),
                    std::path::PathBuf::from(&dest.path),
                )
            };
            ensure!(
                !to.starts_with(&from),
                "Cannot copy a directory into itself"
            );
        }
        if let Some(c) = &dest_conn {
            if c.sftp.metadata(&dest.path).await.is_err() {
                control.io(c.sftp.create_dir(&dest.path)).await?;
            }
        } else {
            control.io(tokio::fs::create_dir_all(&dest.path)).await?;
        }
        let mut total = 0;
        for e in control.io(list(state, &source)).await? {
            ensure!(
                ![".", ".."].contains(&e.name.as_str()) && !e.name.contains(['/', '\\']),
                "Unsafe remote filename"
            );
            let path = if dest.connection == "local" {
                Path::new(&dest.path).join(&e.name).to_string_lossy().into()
            } else {
                format!("{}/{}", dest.path.trim_end_matches('/'), e.name)
            };
            total += Box::pin(copy_inner(
                app,
                state,
                id,
                Endpoint {
                    connection: source.connection.clone(),
                    path: e.path,
                },
                Endpoint {
                    connection: dest.connection.clone(),
                    path,
                },
                overwrite,
                control,
            ))
            .await?;
        }
        return Ok(total);
    }
    let exists = if let Some(c) = &dest_conn {
        control
            .io(c.sftp.symlink_metadata(&dest.path))
            .await
            .is_ok()
    } else {
        tokio::fs::symlink_metadata(&dest.path).await.is_ok()
    };
    ensure!(
        overwrite || !exists,
        "Destination exists; enable overwrite to replace it"
    );
    let staging = format!("{}.termterm-{}.part", dest.path, uuid::Uuid::new_v4());
    let result:Result<u64>=async {
        let mut input=control.io(input_file(&source,&source_conn)).await?;
        let mut output=control.io(output_file(&staging,&dest_conn,false)).await?;
        let mut buf=vec![0;128*1024];
        let mut transferred=0;
        let mut last=std::time::Instant::now();
        loop {
            if control.checkpoint().await? {
                let current=control.io(fingerprint(&source,&source_conn)).await?;
                if current!=metadata {
                    ensure!(!current.0&&!current.1,"Transfer source changed type");
                    drop(output);
                    input=control.io(input_file(&source,&source_conn)).await?;
                    output=control.io(output_file(&staging,&dest_conn,true)).await?;
                    metadata=current;transferred=0;
                    app.emit("transfer-progress",serde_json::json!({"id":id,"transferred":0,"total":metadata.2,"restarted":true}))?;
                }
            }
            let n=control.io(input.read(&mut buf)).await?;
            if n==0{break;}
            control.io(output.write_all(&buf[..n])).await?;
            transferred+=n as u64;
            if last.elapsed().as_millis()>=100 {
                app.emit("transfer-progress",serde_json::json!({"id":id,"transferred":transferred,"total":metadata.2}))?;
                last=std::time::Instant::now();
            }
        }
        control.io(output.flush()).await?;
        control.io(output.shutdown()).await?;
        drop(output);
        control.checkpoint().await?;
        ensure!(control.io(fingerprint(&source,&source_conn)).await?==metadata&&transferred==metadata.2,"Source changed during transfer; retry");
        control.begin_commit().await?;
        // Do not cancel a rename halfway through: it may already have reached the server.
        let committed=commit_file(&staging,&dest, &dest_conn,overwrite,exists).await;
        control.end_commit();
        committed?;
        app.emit("transfer-progress",serde_json::json!({"id":id,"transferred":transferred,"total":metadata.2}))?;
        Ok(transferred)
    }.await;
    if result.is_err() {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            if let Some(c) = &dest_conn {
                let _ = c.sftp.remove_file(&staging).await;
            } else {
                let _ = tokio::fs::remove_file(&staging).await;
            }
        })
        .await;
    }
    result
}
async fn commit_file(
    staging: &str,
    dest: &Endpoint,
    conn: &Option<Arc<SftpConnection>>,
    overwrite: bool,
    exists: bool,
) -> Result<()> {
    if let Some(c) = conn {
        if exists && overwrite {
            let previous = format!("{}.termterm-previous-{}", dest.path, uuid::Uuid::new_v4());
            c.sftp.rename(&dest.path, &previous).await?;
            if let Err(e) = c.sftp.rename(staging, &dest.path).await {
                c.sftp.rename(&previous,&dest.path).await.map_err(|restore|anyhow!("Commit failed: {e}; original retained at {previous}; restore failed: {restore}"))?;
                return Err(e.into());
            }
            let _ = c.sftp.remove_file(&previous).await;
        } else {
            c.sftp.rename(staging, &dest.path).await?;
        }
    } else if overwrite {
        tokio::fs::rename(staging, &dest.path).await?;
    } else {
        tokio::fs::hard_link(staging, &dest.path).await?;
        tokio::fs::remove_file(staging).await?;
    }
    Ok(())
}
