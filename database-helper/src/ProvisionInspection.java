import com.google.gson.*;
import java.sql.*;
import java.util.*;

/** Read-only recovery evidence. Never treats matching names as ownership proof or replays DDL. */
final class ProvisionInspection {
  static Object inspect(Main.Session s,JsonObject p)throws Exception{
    if(!Set.of("id","op","sessionId","name","locale","oracleMode","tablespace","quotaMb","pdbMaxMb").containsAll(p.keySet()))throw new IllegalArgumentException("Beklenmeyen denetim alanı");
    var in=Provisioning.input(p);var state=s.provisioning;if(!state.busy.compareAndSet(false,true))throw new IllegalStateException("Kurulum/denetim sürüyor");
    Connection c=null;boolean slot=false;var sections=new ArrayList<Object>();
    try{
      if(state.closed)throw new IllegalStateException("Bağlantı kapandı");if(!Main.connectionSlots.tryAcquire())throw new IllegalStateException("Bağlantı sınırı");slot=true;c=Main.connect(s.profile);state.active=c;c.setNetworkTimeout(Main.workers,20000);ChangePlans.supported(c,s.engine);
      boolean target=false;
      if(s.engine.equals("postgresql")){
        section(c,sections,"Veritabanı ve sahibi","SELECT datname,pg_get_userbyid(datdba) AS owner,datallowconn,pg_encoding_to_char(encoding) AS encoding,datcollate,datctype FROM pg_database WHERE datname=?",in.name());
        target=Provisioning.scalar(c,"SELECT 1 FROM pg_database WHERE datname=? AND datallowconn",in.name())!=null;
        section(c,sections,"Rol bayrakları","SELECT rolname,rolcanlogin,rolsuper,rolcreatedb,rolcreaterole,rolreplication,rolbypassrls FROM pg_roles WHERE rolname IN (?,?,?,?,?,?) ORDER BY rolname",Provisioning.names(in).toArray(String[]::new));
        section(c,sections,"Üyelik ve grantor","SELECT r.rolname AS role,m.rolname AS member,g.rolname AS grantor,a.admin_option,a.inherit_option,a.set_option FROM pg_auth_members a JOIN pg_roles r ON r.oid=a.roleid JOIN pg_roles m ON m.oid=a.member JOIN pg_roles g ON g.oid=a.grantor WHERE r.rolname IN (?,?,?,?,?,?) OR m.rolname IN (?,?,?,?,?,?) ORDER BY r.rolname,m.rolname",twice(Provisioning.names(in)));
      }else if(s.engine.equals("mssql")){
        if(!"1".equals(Provisioning.scalar(c,"SELECT IS_SRVROLEMEMBER('sysadmin')")))throw new IllegalArgumentException("Tam kurulum denetimi için sysadmin metadata görünürlüğü gerekli");
        section(c,sections,"Veritabanı ve sahibi","SELECT name,SUSER_SNAME(owner_sid) AS owner,state_desc,collation_name FROM sys.databases WHERE name=?",in.name());target=Provisioning.scalar(c,"SELECT 1 FROM sys.databases WHERE name=? AND state=0",in.name())!=null;
        section(c,sections,"Login durumları","SELECT name,is_disabled,default_database_name,type_desc FROM sys.server_principals WHERE name IN (?,?) ORDER BY name",in.name()+"_owner",in.name()+"_appuser");
      }else{
        if(in.oracleMode().equals("pdb")){
          if(!"CDB$ROOT".equals(Provisioning.scalar(c,"SELECT SYS_CONTEXT('USERENV','CON_NAME') FROM dual")))throw new IllegalArgumentException("PDB denetimi CDB$ROOT bağlantısı gerektiriyor");
          section(c,sections,"PDB durumu","SELECT name,open_mode,restricted FROM v$pdbs WHERE name=?",Provisioning.name(in.name(),s.engine));target=Provisioning.scalar(c,"SELECT 1 FROM v$pdbs WHERE name=? AND open_mode='READ WRITE'",Provisioning.name(in.name(),s.engine))!=null;
        }else target=true;
      }
      if(target){
        if(!s.engine.equals("oracle")||in.oracleMode().equals("pdb")){
          c.close();var profile=s.profile.deepCopy();profile.addProperty("database",in.name());
          try{c=Main.connect(profile);state.active=c;c.setNetworkTimeout(Main.workers,20000);}catch(Exception e){sections.add(Map.of("label","Hedef bağlantısı","state","unknown","message","Hedef açılmış olabilir ancak bu yönetim hesabıyla okunamadı; mevcut kullanıcı/servis erişimini doğrulayın","rows",List.of()));return result(s,in,sections);}
        }
        if(s.engine.equals("postgresql")){
          section(c,sections,"Public schema sahibi ve ACL","SELECT nspname,pg_get_userbyid(nspowner) AS owner,nspacl::text AS acl FROM pg_namespace WHERE nspname='public'");
          section(c,sections,"Varsayılan nesne izinleri","SELECT r.rolname AS creator,COALESCE(n.nspname,'all schemas') AS schema,d.defaclobjtype,d.defaclacl::text AS acl FROM pg_default_acl d JOIN pg_roles r ON r.oid=d.defaclrole LEFT JOIN pg_namespace n ON n.oid=d.defaclnamespace WHERE r.rolname=?",in.name()+"_owner");
          section(c,sections,"DB erişim ACL","SELECT datacl::text AS acl FROM pg_database WHERE datname=current_database()");
        }else if(s.engine.equals("mssql")){
          section(c,sections,"DB hesapları ve roller","SELECT name,type_desc,default_schema_name FROM sys.database_principals WHERE name IN (?,?,?,?,?,?,'dbo') ORDER BY name",Provisioning.names(in).toArray(String[]::new));
          section(c,sections,"DB rol üyelikleri","SELECT r.name AS role,m.name AS member FROM sys.database_role_members x JOIN sys.database_principals r ON r.principal_id=x.role_principal_id JOIN sys.database_principals m ON m.principal_id=x.member_principal_id WHERE r.name IN (?,?,?,?,?,?) OR m.name IN (?,?,?,?,?,?)",twice(Provisioning.names(in)));
          section(c,sections,"DB ve schema izinleri","SELECT p.name AS grantee,x.state_desc,x.permission_name,x.class_desc,CASE WHEN x.class=3 THEN SCHEMA_NAME(x.major_id) ELSE NULL END AS schema_name FROM sys.database_permissions x JOIN sys.database_principals p ON p.principal_id=x.grantee_principal_id WHERE p.name IN (?,?,?,?,?,?,'public','guest')",Provisioning.names(in).toArray(String[]::new));
        }else{
          String[] names=Provisioning.names(in).stream().map(n->Provisioning.name(n,s.engine)).toArray(String[]::new);
          section(c,sections,"Schema ve hesap durumları","SELECT username,account_status,default_tablespace FROM dba_users WHERE username IN (?,?,?,?,?,?)",names);
          section(c,sections,"Roller","SELECT role,authentication_type FROM dba_roles WHERE role IN (?,?,?,?,?,?)",names);
          section(c,sections,"Rol üyelikleri","SELECT grantee,granted_role,admin_option,default_role FROM dba_role_privs WHERE grantee IN (?,?,?,?,?,?)",names);
          section(c,sections,"Sistem izinleri","SELECT grantee,privilege,admin_option FROM dba_sys_privs WHERE grantee IN (?,?,?,?,?,?)",names);
          section(c,sections,"Schema kapsamı izinleri","SELECT grantee,schema,privilege,admin_option FROM dba_schema_privs WHERE grantee IN (?,?,?,?,?,?)",names);
        }
      }else sections.add(Map.of("label","Hedef nesneler","state","unknown","message","DB/PDB görünmüyor veya bağlantıya açık değil; alt nesneler okunmadı","rows",List.of()));
      return result(s,in,sections);
    }finally{state.active=null;if(c!=null)try{c.close();}catch(Exception ignored){}if(slot)Main.connectionSlots.release();state.busy.set(false);}
  }
  static String[] twice(List<String> names){var all=new ArrayList<>(names);all.addAll(names);return all.toArray(String[]::new);}
  static void section(Connection c,List<Object> sections,String label,String sql,String...args){
    try(var st=c.prepareStatement(sql)){st.setQueryTimeout(5);st.setMaxRows(501);for(int i=0;i<args.length;i++)st.setString(i+1,args[i]);try(var rs=st.executeQuery()){
      var rows=new ArrayList<Object>();while(rs.next()){if(rows.size()==500)throw new SQLException("Metadata sınırı aşıldı");var row=new LinkedHashMap<String,String>();for(int i=1;i<=rs.getMetaData().getColumnCount();i++)row.put(rs.getMetaData().getColumnLabel(i),rs.getString(i));rows.add(row);}sections.add(Map.of("label",label,"state",rows.isEmpty()?"missing":"observed","rows",rows,"message",rows.isEmpty()?"Görünür kayıt bulunmadı; tamamlandı anlamına gelmez":"Okunan durum; şablona tam uyumluluk garantisi değildir"));
    }}catch(Exception e){sections.add(Map.of("label",label,"state","unknown","message","Metadata okunamadı; yetki/sürüm/bağlantı kontrol edilmeli. "+(e instanceof SQLException x?Objects.toString(x.getSQLState(),"")+"/"+x.getErrorCode():"yerel hata"),"rows",List.of()));}
  }
  static Object result(Main.Session s,Provisioning.Input in,List<Object> sections){return Map.of("state","inspection","name",in.name(),"sections",sections,"message","Salt okunur kurulum denetimi. Aynı adlı nesneler önceki işlemin kanıtı değildir. Otomatik devam, parola sıfırlama ve DROP yapılmadı; eksik/unknown kısımları DBA değerlendirmeli.","expectedSteps",Provisioning.steps(s.engine,in));}
}
