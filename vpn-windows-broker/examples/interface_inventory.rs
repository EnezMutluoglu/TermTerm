//! Read-only native inventory; never loads Wintun or opens the protected journal root.
#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use windows_sys::Win32::{NetworkManagement::IpHelper::{GetIfTable2,FreeMibTable,GetIpInterfaceEntry,InitializeIpInterfaceEntry,MIB_IPINTERFACE_ROW},Networking::WinSock::AF_INET};
    let selected=std::env::args().skip(1).map(|v|v.parse::<u64>()).collect::<Result<Vec<_>,_>>()?;
    let mut table=std::ptr::null_mut();let code=unsafe {GetIfTable2(&mut table)};
    anyhow::ensure!(code == 0,"GetIfTable2: {code}");
    for row in unsafe {std::slice::from_raw_parts((*table).Table.as_ptr(),(*table).NumEntries as usize)} {
        let luid=unsafe {row.InterfaceLuid.Value};
        if !selected.is_empty() && !selected.contains(&luid) {continue;}
        let text=|v:&[u16]|String::from_utf16_lossy(&v[..v.iter().position(|c|*c == 0).unwrap_or(v.len())]);
        let mut ip=MIB_IPINTERFACE_ROW::default();unsafe {InitializeIpInterfaceEntry(&mut ip);};ip.Family=AF_INET;ip.InterfaceLuid=row.InterfaceLuid;
        let ipv4_code=unsafe {GetIpInterfaceEntry(&mut ip)};
        use windows_sys::Win32::Devices::DeviceAndDriverInstallation::*;
        let guid=termterm_vpn_windows_broker::windows::wintun::from_guid(row.InterfaceGuid);
        let instance=format!("SWD\\WINTUN\\{{{}}}",guid.to_string().to_uppercase()).encode_utf16().chain([0]).collect::<Vec<_>>();
        let mut node=0;let phantom_code=unsafe {CM_Locate_DevNodeW(&mut node,instance.as_ptr(),CM_LOCATE_DEVNODE_PHANTOM)};
        let mut flags=0;let mut problem=0;let status_code=if phantom_code==0 {unsafe {CM_Get_DevNode_Status(&mut flags,&mut problem,node,0)}} else {phantom_code};
        let mut normal=0;let normal_code=unsafe {CM_Locate_DevNodeW(&mut normal,instance.as_ptr(),CM_LOCATE_DEVNODE_NORMAL)};
        println!("{}",serde_json::json!({"guid":guid,"luid":luid.to_string(),"index":row.InterfaceIndex,"alias":text(&row.Alias),"description":text(&row.Description),"type":row.Type,"operStatus":row.OperStatus,"adminStatus":row.AdminStatus,"mediaConnectState":row.MediaConnectState,"statusFlags":row.InterfaceAndOperStatusFlags._bitfield,"mtu":row.Mtu,"ipv4":{"code":ipv4_code,"nlMtu":ip.NlMtu,"sitePrefixLength":ip.SitePrefixLength,"family":ip.Family,"index":ip.InterfaceIndex},"pnp":{"phantomLocateCode":phantom_code,"normalLocateCode":normal_code,"statusCode":status_code,"statusFlags":flags,"problem":problem}}));
    }
    unsafe {FreeMibTable(table.cast());}
    Ok(())
}
#[cfg(not(windows))]
fn main() { eprintln!("Windows interface inventory requires Windows"); }
