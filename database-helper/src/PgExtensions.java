import com.google.gson.*;
import java.sql.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;

/** Provider evidence is advisory; no provider/admin role is ever granted to an application owner. */
final class PgExtensions {
  static final class State {final AtomicBoolean busy=new AtomicBoolean();volatile boolean closed;volatile Connection active;Draft draft;}
  record Draft(String id,long created,String hash,String extension,String version,String schema,String baseline,String sql){}
  static List<Map<String,Object>> rows(Connection c,String sql)throws Exception{try(var st=c.createStatement()){st.setQueryTimeout(10);return DbTools.objects(st.executeQuery(sql));}}
  static Map<String,Object> environment(Connection c)throws Exception{
    var r=new LinkedHashMap<String,Object>();
    r.put("identity",rows(c,"SELECT current_database() AS database,current_user AS username,version() AS version,rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user"));
    r.put("providerRoles",rows(c,"SELECT rolname,pg_has_role(current_user,oid,'MEMBER') AS member,pg_has_role(current_user,oid,'USAGE') AS usable FROM pg_roles WHERE rolname IN ('azure_pg_admin','rds_superuser','rds_extension','root') ORDER BY rolname"));
    r.put("settings",rows(c,"SELECT name,setting,pending_restart FROM pg_settings WHERE name IN ('azure.extensions','rds.extensions','rds.allowed_extensions','rds.allowed_delegated_extensions','shared_preload_libraries') ORDER BY name"));
    r.put("extensions",rows(c,"SELECT a.name,a.default_version,a.installed_version,v.trusted,v.superuser,v.schema AS required_schema,v.requires::text FROM pg_available_extensions a JOIN pg_available_extension_versions v ON v.name=a.name AND v.version=a.default_version ORDER BY a.name"));
    r.put("schemas",rows(c,"SELECT nspname FROM pg_namespace WHERE nspname NOT LIKE 'pg_%' AND nspname<>'information_schema' AND has_schema_privilege(oid,'CREATE') ORDER BY nspname"));
    r.put("validation","Sağlayıcı rolü/parametresi bir ipucudur; gerçek Azure/AWS/Huawei ortamı doğrulanmadı. Yetki yükseltme otomatik yapılmaz.");
    return r;
  }
  static String snapshot(Connection c,String ext,String schema)throws Exception{
    if(c.getMetaData().getDatabaseMajorVersion()!=18)throw new IllegalArgumentException("Extension uygulaması bu geliştirmede PostgreSQL 18 ile doğrulandı");
    if(ext.length()>128||schema.length()>128)throw new IllegalArgumentException("Geçersiz extension/schema");
    if(Provisioning.scalar(c,"SELECT 1 FROM pg_extension WHERE extname=?",ext)!=null)throw new IllegalArgumentException("Extension zaten kurulu; mevcut sürüm değiştirilmez");
    if(!"t".equals(Provisioning.scalar(c,"SELECT has_database_privilege(current_database(),'CREATE') AND has_schema_privilege(?,'CREATE')",schema)))throw new IllegalArgumentException("DB ve hedef schema üzerinde CREATE gerekli");
    if(schema.startsWith("pg_")||schema.equals("information_schema"))throw new IllegalArgumentException("Sistem schema'sı kullanılamaz");
    var extension=ChangePlans.one(c,"SELECT a.name,a.default_version,v.trusted::text,v.superuser::text,v.schema,v.requires::text FROM pg_available_extensions a JOIN pg_available_extension_versions v ON v.name=a.name AND v.version=a.default_version WHERE a.name=?",ext);
    for(String setting:List.of("azure.extensions","rds.allowed_extensions")){
      String allowed=Provisioning.scalar(c,"SELECT setting FROM pg_settings WHERE name=?",setting);
      if(allowed!=null&&!Arrays.stream(allowed.split(",")).map(String::trim).anyMatch(v->v.equals("*")||v.equals(ext)))throw new IllegalArgumentException(ext+" sunucunun "+setting+" izin listesinde yok");
    }
    if(extension.get("schema")!=null&&!extension.get("schema").equals(schema))throw new IllegalArgumentException("Extension şu schema'yı gerektiriyor: "+extension.get("schema"));
    if(!extension.get("default_version").matches("[A-Za-z0-9_.+-]{1,64}"))throw new IllegalArgumentException("Desteklenmeyen extension sürüm biçimi");
    if(Provisioning.scalar(c,"SELECT 1 FROM pg_available_extension_versions v CROSS JOIN LATERAL unnest(v.requires) d WHERE v.name=? AND v.version=? AND NOT EXISTS(SELECT 1 FROM pg_extension e WHERE e.extname=d)",ext,extension.get("default_version"))!=null)throw new IllegalArgumentException("Eksik bağımlılık var; otomatik CASCADE kullanılmaz");
    // Server-side extension hooks remain authoritative (Azure allowlist, RDS delegation, Huawei root).
    var evidence=environment(c);
    var memberships=rows(c,"SELECT r.rolname AS role,m.rolname AS member,g.rolname AS grantor,a.admin_option,a.inherit_option,a.set_option FROM pg_auth_members a JOIN pg_roles r ON r.oid=a.roleid JOIN pg_roles m ON m.oid=a.member JOIN pg_roles g ON g.oid=a.grantor ORDER BY 1,2,3");
    return Main.JSON.toJson(List.of(extension,evidence,memberships));
  }
  static void close(Main.Session s){State st=s.extensions;st.closed=true;st.draft=null;Connection c=st.active;if(c!=null)try{c.abort(Main.workers);}catch(Exception ignored){}}
  static Object execute(Main.Session s,JsonObject p)throws Exception{
    if(!s.engine.equals("postgresql"))throw new IllegalArgumentException("Bu alan PostgreSQL içindir");
    String op=Main.str(p,"op","");var allowed=new HashSet<>(Set.of("id","op","sessionId"));if(op.equals("extensionPreview"))allowed.addAll(Set.of("extension","schema"));else if(!op.equals("extensionEnvironment"))allowed.addAll(Set.of("planId","hash"));if(!allowed.containsAll(p.keySet()))throw new IllegalArgumentException("Beklenmeyen alan");
    State state=s.extensions;if(!state.busy.compareAndSet(false,true))throw new IllegalStateException("Extension işlemi sürüyor");
    Connection c=null;boolean slot=false,commitSent=false,dispatched=false;Draft draft=null;
    try{
      if(state.closed)throw new IllegalStateException("Bağlantı kapandı");
      if(op.equals("extensionDiscard")){if(state.draft!=null&&state.draft.id.equals(Main.str(p,"planId","")))state.draft=null;return Map.of("discarded",true);}
      if(op.equals("extensionApply")){draft=state.draft;if(draft==null||ChangePlans.expired(draft.created,System.nanoTime())||!draft.id.equals(Main.str(p,"planId",""))||!draft.hash.equals(Main.str(p,"hash","")))throw new IllegalArgumentException("Plan geçersiz/süresi doldu");state.draft=null;}
      else if(op.equals("extensionPreview"))state.draft=null;
      if(!Main.connectionSlots.tryAcquire())throw new IllegalStateException("Bağlantı sınırı");slot=true;c=Main.connect(s.profile);state.active=c;c.setNetworkTimeout(Main.workers,45000);
      if(op.equals("extensionEnvironment"))return environment(c);
      String ext=draft==null?Main.str(p,"extension",""):draft.extension,schema=draft==null?Main.str(p,"schema","public"):draft.schema;
      c.setAutoCommit(false);try(var st=c.createStatement()){st.execute("SET LOCAL statement_timeout='30s'");st.execute("SET LOCAL lock_timeout='5s'");st.execute("SET LOCAL search_path=pg_catalog");st.execute("SET LOCAL standard_conforming_strings=on");}
      String baseline=snapshot(c,ext,schema);
      if(op.equals("extensionPreview")){
        String version=Provisioning.scalar(c,"SELECT default_version FROM pg_available_extensions WHERE name=?",ext);
        String sql="CREATE EXTENSION "+DbTools.q(ext,"postgresql")+" WITH SCHEMA "+DbTools.q(schema,"postgresql")+" VERSION "+Provisioning.lit(version);
        String hash=ChangePlans.hash(baseline+sql);draft=new Draft(UUID.randomUUID().toString(),System.nanoTime(),hash,ext,version,schema,baseline,sql);c.rollback();if(state.closed)throw new IllegalStateException("Bağlantı kapandı");state.draft=draft;
        return Map.of("planId",draft.id,"hash",hash,"sql",sql,"database",Main.str(s.profile,"database",""),"username",Main.str(s.profile,"username",""),"host",Main.str(s.profile,"host",""),"expiresInSeconds",300,"roleChanges",List.of());
      }
      if(!baseline.equals(draft.baseline))return Map.of("state","conflict","message","Extension, izin veya sağlayıcı ayarları değişti; yeniden önizleyin");
      if(state.closed)throw new IllegalStateException("Bağlantı kapandı");
      try(var st=c.createStatement()){st.setQueryTimeout(30);dispatched=true;st.execute(draft.sql);}
      if(!draft.version.equals(Provisioning.scalar(c,"SELECT extversion FROM pg_extension WHERE extname=?",ext)))throw new IllegalStateException("Extension sürümü doğrulanamadı");
      if(state.closed)throw new IllegalStateException("Bağlantı kapandı");commitSent=true;c.commit();return Map.of("state","succeeded","message","Extension doğrulandı. Rol üyelikleri değiştirilmedi.");
    }catch(Exception e){
      if(op.equals("extensionApply")){boolean rolled=false;if(c!=null&&!commitSent)try{c.rollback();rolled=true;}catch(Exception ignored){}
        return Map.of("state",rolled||!dispatched?"failed":"unknown_outcome","message",rolled||!dispatched?"İşlem uygulanmadı/geri alındı; yetki/allowlist/bağımlılık koşullarını kontrol edin. "+Objects.toString(e.getMessage(),""):"Sonuç doğrulanamadı; extension kataloğunu kontrol edin. Otomatik tekrar yok.");}
      throw e;
    }finally{state.active=null;if(c!=null)try{if(!c.getAutoCommit())c.rollback();c.close();}catch(Exception ignored){}if(slot)Main.connectionSlots.release();state.busy.set(false);}
  }
}
