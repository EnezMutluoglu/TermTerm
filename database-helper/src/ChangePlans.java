import com.google.gson.*;
import java.sql.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;

/** Fixed, reviewed maintenance operations. Renderer SQL is never accepted. */
final class ChangePlans {
  static final long TTL=300_000_000_000L;
  static final Map<String,String> ACTIONS=Map.of("postgresql","PG-OPS-004","mssql","MS-OPS-003","oracle","OR-OPS-003");
  static final class State {
    final AtomicBoolean busy=new AtomicBoolean(); volatile boolean closed; volatile Connection active;
    Draft draft;
  }
  record Target(String schema,String table,String statistic) {}
  record Snapshot(String identity,List<Map<String,Object>> columns,Map<String,String> evidence) {}
  record Draft(String id,long created,Target target,String action,String sql,Snapshot before,String hash) {}
  static boolean expired(long created,long now){return now-created>=TTL;}
  static String hash(String text)throws Exception{return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(text.getBytes(StandardCharsets.UTF_8)));}
  static void close(Main.Session s){State st=s.maintenance;st.closed=true;synchronized(st){st.draft=null;}Connection c=st.active;if(c!=null)try{c.abort(Main.workers);}catch(Exception ignored){}}
  static Map<String,String> one(Connection c,String sql,String...args)throws SQLException{
    try(var st=c.prepareStatement(sql)){st.setQueryTimeout(5);for(int i=0;i<args.length;i++)st.setString(i+1,args[i]);try(var rs=st.executeQuery()){
      if(!rs.next())throw new IllegalArgumentException("Hedef görünmüyor veya bu bakım türüne uygun değil");
      var row=new LinkedHashMap<String,String>();for(int i=1;i<=rs.getMetaData().getColumnCount();i++)row.put(rs.getMetaData().getColumnLabel(i).toLowerCase(Locale.ROOT),rs.getString(i));
      if(rs.next())throw new IllegalArgumentException("Hedef tekil değil");return row;
    }}
  }
  static void supported(Connection c,String engine)throws SQLException{
    int major=c.getMetaData().getDatabaseMajorVersion();
    if(!((engine.equals("postgresql")&&major==18)||(engine.equals("mssql")&&major==16)||(engine.equals("oracle")&&major==23)))throw new IllegalArgumentException("Bakım bu motor/sürümde henüz doğrulanmadı");
  }
  static Snapshot snapshot(Connection c,String engine,Target t)throws Exception{
    supported(c,engine);DbTools.q(t.schema,engine);DbTools.q(t.table,engine);
    Map<String,String> row;
    if(engine.equals("postgresql")){
      row=one(c,"SELECT c.oid::text AS object_id,c.relfilenode::text AS storage_id,pg_get_userbyid(c.relowner) AS owner,c.reltuples::text AS estimated_rows,s.last_analyze::text AS last_analyzed,has_table_privilege(c.oid,'MAINTAIN')::text AS permitted FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace LEFT JOIN pg_stat_all_tables s ON s.relid=c.oid WHERE n.nspname=? AND c.relname=? AND c.relkind='r' AND NOT EXISTS(SELECT 1 FROM pg_inherits i WHERE i.inhrelid=c.oid OR i.inhparent=c.oid)",t.schema,t.table);
      if(!"true".equals(row.remove("permitted")))throw new IllegalArgumentException("Tabloda MAINTAIN yetkisi gerekli");
    }else if(engine.equals("mssql")){
      row=one(c,"SELECT t.object_id,CONVERT(varchar(33),t.create_date,126) AS created_at,s.stats_id,s.name AS statistics_name,CONVERT(varchar(33),p.last_updated,126) AS last_analyzed,CONVERT(varchar(30),p.rows) AS estimated_rows,HAS_PERMS_BY_NAME(QUOTENAME(sc.name)+'.'+QUOTENAME(t.name),'OBJECT','ALTER') AS permitted FROM sys.tables t JOIN sys.schemas sc ON sc.schema_id=t.schema_id JOIN sys.stats s ON s.object_id=t.object_id OUTER APPLY sys.dm_db_stats_properties(s.object_id,s.stats_id) p WHERE sc.name=? AND t.name=? AND s.name=? AND t.is_memory_optimized=0 AND t.is_external=0",t.schema,t.table,t.statistic);
      if(!"1".equals(row.remove("permitted")))throw new IllegalArgumentException("Tabloda ALTER yetkisi gerekli");
    }else{
      row=one(c,"SELECT o.object_id,TO_CHAR(o.created,'YYYY-MM-DD HH24:MI:SS') AS created_at,TO_CHAR(o.last_ddl_time,'YYYY-MM-DD HH24:MI:SS') AS ddl_at,TO_CHAR(s.last_analyzed,'YYYY-MM-DD HH24:MI:SS') AS last_analyzed,TO_CHAR(s.num_rows) AS estimated_rows,s.stattype_locked AS locked_stats,CASE WHEN USER=t.owner OR EXISTS(SELECT 1 FROM session_privs WHERE privilege='ANALYZE ANY') THEN '1' ELSE '0' END AS permitted FROM all_tables t JOIN all_objects o ON o.owner=t.owner AND o.object_name=t.table_name AND o.object_type='TABLE' JOIN all_tab_statistics s ON s.owner=t.owner AND s.table_name=t.table_name AND s.partition_name IS NULL AND s.subpartition_name IS NULL WHERE t.owner=? AND t.table_name=? AND t.partitioned='NO' AND t.temporary='N' AND t.nested='NO' AND t.iot_type IS NULL",t.schema,t.table);
      if(!"1".equals(row.remove("permitted")))throw new IllegalArgumentException("Tablo sahipliği veya ANALYZE ANY gerekli");
      if(row.get("locked_stats")!=null)throw new IllegalArgumentException("İstatistik kilitli; otomatik kilit kaldırılmaz");
      var prefs=one(c,"SELECT DBMS_STATS.GET_PREFS('PUBLISH',?,?) AS publish,DBMS_STATS.GET_PREFS('INCREMENTAL',?,?) AS incremental FROM dual",DbTools.q(t.schema,engine),DbTools.q(t.table,engine),DbTools.q(t.schema,engine),DbTools.q(t.table,engine));
      if(!"TRUE".equalsIgnoreCase(prefs.get("publish"))||!"FALSE".equalsIgnoreCase(prefs.get("incremental")))throw new IllegalArgumentException("Pending/incremental istatistik politikası ayrı plan gerektiriyor");
    }
    var detail=DbTools.detail(c,engine,t.schema,t.table);
    String identity=Main.JSON.toJson(List.of(engine,c.getMetaData().getDatabaseProductVersion(),Objects.toString(c.getCatalog(),""),c.getMetaData().getUserName()));
    return new Snapshot(identity,(List<Map<String,Object>>)detail.get("columns"),row);
  }
  static String sql(String engine,Target t){
    String target=DbTools.q(t.schema,engine)+"."+DbTools.q(t.table,engine);
    return switch(engine){
      case "postgresql" -> "ANALYZE VERBOSE "+target;
      case "mssql" -> "UPDATE STATISTICS "+target+" "+DbTools.q(t.statistic,engine);
      default -> "BEGIN DBMS_STATS.GATHER_TABLE_STATS(ownname => ?, tabname => ?, estimate_percent => DBMS_STATS.AUTO_SAMPLE_SIZE, method_opt => 'FOR ALL COLUMNS SIZE AUTO', degree => 1, cascade => DBMS_STATS.AUTO_CASCADE, no_invalidate => DBMS_STATS.AUTO_INVALIDATE); END;";
    };
  }
  static Object execute(Main.Session session,JsonObject p)throws Exception{
    State state=session.maintenance;String op=Main.str(p,"op","");
    Set<String> allowed=new HashSet<>(Set.of("id","op","sessionId"));
    allowed.addAll(op.equals("changeApply")?Set.of("planId","hash"):op.equals("changeDiscard")?Set.of("planId"):Set.of("actionId","schema","table","statistic"));
    if(!allowed.containsAll(p.keySet()))throw new IllegalArgumentException("Beklenmeyen plan alanı; SQL yalnız sunucuda oluşturulur");
    if(!state.busy.compareAndSet(false,true))throw new IllegalStateException("Bakım işlemi sürüyor");
    Connection c=null;boolean slot=false,started=false;
    try{
      if(state.closed)throw new IllegalStateException("Bağlantı kapandı");
      if(op.equals("changeDiscard")){synchronized(state){if(state.draft!=null&&state.draft.id.equals(Main.str(p,"planId","")))state.draft=null;}return Map.of("discarded",true);}
      Draft draft=null;
      if(op.equals("changeApply"))synchronized(state){
        draft=state.draft;
        if(draft==null||expired(draft.created,System.nanoTime())||!draft.id.equals(Main.str(p,"planId",""))||!draft.hash.equals(Main.str(p,"hash","")))throw new IllegalArgumentException("Önizleme süresi doldu, değişti veya başka oturuma ait");
        state.draft=null; // One attempt only, including failure. Never retry maintenance silently.
      }else if(op.equals("changePreview"))synchronized(state){state.draft=null;}
      if(!Main.connectionSlots.tryAcquire())throw new IllegalStateException("Bağlantı sınırı");slot=true;
      c=Main.connect(session.profile);state.active=c;c.setAutoCommit(true);c.setNetworkTimeout(Main.workers,40000);
      if(state.closed)throw new IllegalStateException("Bağlantı kapandı");
      if(op.equals("changeOptions")){
        supported(c,session.engine);var names=new ArrayList<String>();
        if(session.engine.equals("mssql"))try(var st=c.prepareStatement("SELECT s.name FROM sys.stats s JOIN sys.tables t ON t.object_id=s.object_id JOIN sys.schemas sc ON sc.schema_id=t.schema_id WHERE sc.name=? AND t.name=? ORDER BY s.name")){
          st.setQueryTimeout(5);st.setMaxRows(1001);st.setString(1,Main.str(p,"schema",""));st.setString(2,Main.str(p,"table",""));try(var rs=st.executeQuery()){while(rs.next())names.add(rs.getString(1));}
          if(names.size()>1000)throw new IllegalArgumentException("İstatistik listesi sınırı aşıldı");
        }return Map.of("statistics",names,"actionId",ACTIONS.get(session.engine));
      }
      if(op.equals("changePreview")){
        String action=Main.str(p,"actionId","");if(!Objects.equals(ACTIONS.get(session.engine),action))throw new IllegalArgumentException("Bu bakım işlemi etkin değil");
        Target t=new Target(Main.str(p,"schema",""),Main.str(p,"table",""),Main.str(p,"statistic",""));
        if(!session.engine.equals("mssql")&&!t.statistic.isEmpty())throw new IllegalArgumentException("Bu işlem tek istatistik parametresi kabul etmez");
        Snapshot before=snapshot(c,session.engine,t);String sql=sql(session.engine,t);
        String digest=hash(Main.JSON.toJson(List.of(action,t,sql,before,Main.str(session.profile,"host",""),Main.integer(session.profile,"port",0),Main.str(session.profile,"database",""))));
        draft=new Draft(UUID.randomUUID().toString(),System.nanoTime(),t,action,sql,before,digest);
        synchronized(state){if(state.closed)throw new IllegalStateException("Bağlantı kapandı");state.draft=draft;}
        var connection=Map.of("host",Main.str(session.profile,"host",""),"port",Main.integer(session.profile,"port",session.engine.equals("postgresql")?5432:session.engine.equals("mssql")?1433:1521),"database",Main.str(session.profile,"database",""),"username",Main.str(session.profile,"username",""));
        return Map.of("planId",draft.id,"hash",digest,"sql",sql,"target",t,"before",before.evidence,"expiresInSeconds",300,"actionId",action,"connection",connection,"effect","İstatistikler ve sorgu planları değişebilir. Ayrı autocommit bağlantısı; geri alma garantisi yok. Onay ile uygulama arasındaki DDL yarışı tamamen engellenmez.");
      }
      if(!op.equals("changeApply"))throw new IllegalArgumentException("Geçersiz bakım işlemi");
      Snapshot current=snapshot(c,session.engine,draft.target);
      if(!Main.JSON.toJson(current).equals(Main.JSON.toJson(draft.before)))return Map.of("state","conflict","message","Hedef, tablo yapısı veya istatistik bilgisi değişti; yeni önizleme gerekli");
      try(var st=c.createStatement()){st.setQueryTimeout(5);if(session.engine.equals("postgresql"))st.execute("SET lock_timeout='5s'");else if(session.engine.equals("mssql"))st.execute("SET LOCK_TIMEOUT 5000");}
      if(state.closed)throw new IllegalStateException("Bağlantı kapandı");
      var warnings=new ArrayList<String>();
      try(var st=c.prepareStatement(draft.sql)){
        st.setQueryTimeout(30);
        if(session.engine.equals("oracle")){st.setString(1,DbTools.q(draft.target.schema,"oracle"));st.setString(2,DbTools.q(draft.target.table,"oracle"));}
        started=true;st.execute();for(SQLWarning w=st.getWarnings();w!=null&&warnings.size()<20;w=w.getNextWarning()){
          if(w instanceof org.postgresql.util.PSQLWarning pg && pg.getServerErrorMessage()!=null && Set.of("INFO","NOTICE","LOG","DEBUG").contains(pg.getServerErrorMessage().getSeverity()))continue;
          warnings.add(Objects.toString(w.getMessage(),"Uyarı"));
        }
      }
      Snapshot after=snapshot(c,session.engine,draft.target);
      return Map.of("state",warnings.isEmpty()?"succeeded":"partial","message",warnings.isEmpty()?"Sunucu işlemi tamamladı; performans iyileşmesi garanti edilmez. İstatistik zamanının görünmesi gecikebilir.":"Sunucu uyarı döndürdü; sonucu inceleyin, otomatik tekrar yok.","before",draft.before.evidence,"after",after.evidence,"warnings",warnings,"planId",draft.id);
    }catch(Exception e){
      if(op.equals("changeApply"))return Map.of("state",started?"unknown_outcome":"failed","message",started?"İşlem gönderildi ancak sonuç bütünüyle doğrulanamadı. Yeniden çalıştırmadan sunucu durumunu kontrol edin.":Objects.toString(e.getMessage(),"Ön kontrol başarısız"));
      throw e;
    }finally{state.active=null;if(c!=null)try{c.close();}catch(Exception ignored){}if(slot)Main.connectionSlots.release();state.busy.set(false);}
  }
}
