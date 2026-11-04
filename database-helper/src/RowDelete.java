import com.google.gson.*;
import java.sql.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;

/** One primary-key row only. Reject visible fan-out and triggers; never accept renderer SQL. */
final class RowDelete {
  static final class State {final AtomicBoolean busy=new AtomicBoolean();volatile boolean closed;volatile Connection active;Draft draft;}
  record Draft(String id,long created,String hash,RowEditor.Shape shape,JsonObject keys,JsonObject original,String objectId,String sql){}
  static String guard(Connection c,String engine,String schema,String table)throws Exception{
    return guard(c,engine,schema,table,"DELETE");
  }
  static String guard(Connection c,String engine,String schema,String table,String permission)throws Exception{
    if(!Set.of("INSERT","DELETE").contains(permission))throw new IllegalArgumentException("Geçersiz yetki");
    ChangePlans.supported(c,engine);
    Map<String,String> m;
    if(engine.equals("postgresql"))m=ChangePlans.one(c,"SELECT c.oid::text AS object_id,has_table_privilege(c.oid,'"+permission+"')::text AS permitted,(SELECT count(*) FROM pg_trigger t WHERE t.tgrelid=c.oid AND NOT t.tgisinternal AND t.tgenabled<>'D')::text AS triggers,(SELECT count(*) FROM pg_rewrite r WHERE r.ev_class=c.oid)::text AS rules,(SELECT count(*) FROM pg_constraint f WHERE f.confrelid=c.oid AND f.contype='f' AND f.confdeltype IN ('c','n','d'))::text AS cascading FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=? AND c.relname=?",schema,table);
    else if(engine.equals("mssql")){
      if(!"1".equals(Provisioning.scalar(c,"SELECT HAS_PERMS_BY_NAME(DB_NAME(),'DATABASE','VIEW DEFINITION')")))throw new IllegalArgumentException("İşlem etkisini görmek için DB VIEW DEFINITION gerekli");
      m=ChangePlans.one(c,"SELECT CONVERT(varchar(30),t.object_id) AS object_id,CASE WHEN HAS_PERMS_BY_NAME(QUOTENAME(s.name)+'.'+QUOTENAME(t.name),'OBJECT','"+permission+"')=1 THEN 'true' ELSE 'false' END AS permitted,(SELECT COUNT(*) FROM sys.triggers tr WHERE tr.parent_id=t.object_id AND tr.is_disabled=0) AS triggers,CAST(t.temporal_type+t.is_memory_optimized AS int) AS rules,(SELECT COUNT(*) FROM sys.foreign_keys f WHERE f.referenced_object_id=t.object_id AND f.is_disabled=0 AND f.delete_referential_action<>0) AS cascading FROM sys.tables t JOIN sys.schemas s ON s.schema_id=t.schema_id WHERE s.name=? AND t.name=?",schema,table);
    }else{
      // DBA catalog reads intentionally fail closed when cross-schema references are not visible.
      m=ChangePlans.one(c,"SELECT TO_CHAR(o.object_id) AS object_id,CASE WHEN USER=o.owner OR EXISTS(SELECT 1 FROM session_privs WHERE privilege='"+permission+" ANY TABLE') OR EXISTS(SELECT 1 FROM user_tab_privs_recd WHERE owner=o.owner AND table_name=o.object_name AND privilege='"+permission+"') THEN 'true' ELSE 'false' END AS permitted,(SELECT COUNT(*) FROM dba_triggers tr WHERE tr.table_owner=o.owner AND tr.table_name=o.object_name AND tr.status='ENABLED') AS triggers,0 AS rules,(SELECT COUNT(*) FROM dba_constraints f JOIN dba_constraints pk ON pk.owner=f.r_owner AND pk.constraint_name=f.r_constraint_name WHERE pk.owner=o.owner AND pk.table_name=o.object_name AND f.constraint_type='R' AND f.status='ENABLED' AND f.delete_rule<>'NO ACTION') AS cascading FROM all_objects o WHERE o.owner=? AND o.object_name=? AND o.object_type='TABLE'",schema,table);
    }
    if(!"true".equals(m.get("permitted")))throw new IllegalArgumentException("Tabloda "+permission+" yetkisi gerekli");
    if(!"0".equals(m.get("triggers"))||!"0".equals(m.get("rules")))throw new IllegalArgumentException("Trigger/rule/temporal tablosunda işlem bu aşamada desteklenmiyor");
    if(permission.equals("DELETE")&&!"0".equals(m.get("cascading")))throw new IllegalArgumentException("CASCADE/SET NULL/SET DEFAULT ilişkisi başka satırları etkileyebilir; otomatik silme kapalı");
    return m.get("object_id");
  }
  static void close(Main.Session s){State st=s.deletions;st.closed=true;st.draft=null;Connection c=st.active;if(c!=null)try{c.abort(Main.workers);}catch(Exception ignored){}}
  static Object execute(Main.Session s,JsonObject p)throws Exception{
    String op=Main.str(p,"op","");var fields=new HashSet<>(Set.of("id","op","sessionId"));fields.addAll(op.equals("rowDeletePreview")?Set.of("schema","table","keys"):Set.of("planId","hash"));if(!fields.containsAll(p.keySet()))throw new IllegalArgumentException("Beklenmeyen silme alanı");
    State state=s.deletions;if(!state.busy.compareAndSet(false,true))throw new IllegalStateException("Silme işlemi sürüyor");
    Connection c=null;boolean slot=false,writeStarted=false,commitStarted=false;Draft d=null;
    try{
      if(state.closed)throw new IllegalStateException("Bağlantı kapandı");
      if(op.equals("rowDeleteDiscard")){if(state.draft!=null&&state.draft.id.equals(Main.str(p,"planId","")))state.draft=null;return Map.of("discarded",true);}
      if(op.equals("rowDeleteApply")){d=state.draft;if(d==null||ChangePlans.expired(d.created,System.nanoTime())||!d.id.equals(Main.str(p,"planId",""))||!d.hash.equals(Main.str(p,"hash","")))throw new IllegalArgumentException("Silme planı geçersiz/süresi doldu");state.draft=null;}else state.draft=null;
      if(!Main.connectionSlots.tryAcquire())throw new IllegalStateException("Bağlantı sınırı");slot=true;c=Main.connect(s.profile);state.active=c;c.setNetworkTimeout(Main.workers,20000);
      String schema=d==null?Main.str(p,"schema",""):d.shape.schema(),table=d==null?Main.str(p,"table",""):d.shape.table();
      if(d!=null){
        c.setTransactionIsolation(Connection.TRANSACTION_READ_COMMITTED);c.setAutoCommit(false);String target=RowEditor.target(d.shape,s.engine);
        try(var st=c.createStatement()){st.setQueryTimeout(10);
          if(s.engine.equals("postgresql")){st.execute("SET LOCAL lock_timeout='5s'");st.execute("SET LOCAL statement_timeout='10s'");st.execute("LOCK TABLE "+target+" IN SHARE ROW EXCLUSIVE MODE");}
          else if(s.engine.equals("mssql")){st.execute("SET LOCK_TIMEOUT 5000");st.execute("SELECT TOP(1) 1 FROM "+target+" WITH (TABLOCKX,HOLDLOCK)");}
          else st.execute("LOCK TABLE "+target+" IN EXCLUSIVE MODE WAIT 5");
        }
      }
      String objectId=guard(c,s.engine,schema,table);
      var shape=RowEditor.shape(c,s.engine,schema,table,DbTools.detail(c,s.engine,schema,table));
      JsonObject keys=d==null?p.getAsJsonObject("keys"):d.keys;
      if(keys==null||!keys.keySet().equals(new HashSet<>(shape.keys())))throw new IllegalArgumentException("Birincil anahtar alanları eşleşmiyor");
      JsonObject row=RowEditor.read(c,s.engine,shape,keys,d!=null);
      if(d==null){
        if(row==null)throw new IllegalArgumentException("Satır görünmüyor; yenileyin");
        String sql="DELETE FROM "+RowEditor.target(shape,s.engine)+" WHERE "+RowEditor.where(shape,s.engine);
        String hash=ChangePlans.hash(Main.JSON.toJson(List.of(s.profile.get("host"),s.profile.get("database"),c.getMetaData().getUserName(),objectId,shape.signature(),keys,row,sql)));
        d=new Draft(UUID.randomUUID().toString(),System.nanoTime(),hash,shape,keys.deepCopy(),row.deepCopy(),objectId,sql);if(state.closed)throw new IllegalStateException("Bağlantı kapandı");state.draft=d;
        return Map.of("planId",d.id,"hash",hash,"sql",sql,"schema",schema,"table",table,"keys",keys,"original",row,"expiresInSeconds",300,"connection",Map.of("host",Main.str(s.profile,"host",""),"database",Main.str(s.profile,"database",""),"username",c.getMetaData().getUserName()));
      }
      if(!objectId.equals(d.objectId)||!shape.signature().equals(d.shape.signature())||!d.original.equals(row)){c.rollback();return Map.of("state","conflict","message","Satır veya tablo değişti; silinmedi. Yeni önizleme açın.","rolledBack",true);}
      if(state.closed)throw new IllegalStateException("Bağlantı kapandı");
      try(var st=c.prepareStatement(d.sql)){st.setQueryTimeout(10);int i=1;for(String key:shape.keys())RowEditor.bind(st,i++,RowEditor.column(shape,key),keys.get(key));writeStarted=true;if(st.executeUpdate()!=1)throw new SQLException("Tam olarak bir satır silinmedi");}
      if(RowEditor.read(c,s.engine,shape,keys,false)!=null)throw new SQLException("Silme sonrası kontrol başarısız");
      if(state.closed)throw new IllegalStateException("Bağlantı kapandı");commitStarted=true;c.commit();return Map.of("state","succeeded","affectedRows",1,"message","1 satır silindi ve commit edildi.");
    }catch(Exception e){
      if(!op.equals("rowDeleteApply"))throw e;boolean rolled=false;if(c!=null)try{c.rollback();rolled=true;}catch(Exception ignored){}
      boolean unknown=commitStarted||(writeStarted&&!rolled);return Map.of("state",unknown?"unknown_outcome":"failed","rolledBack",rolled&&!unknown,"message",unknown?"Silme sonucu doğrulanamadı; yeni bağlantıdan kontrol edin. Otomatik tekrar yok.":Objects.toString(e.getMessage(),"Silme başarısız"));
    }finally{state.active=null;if(c!=null)try{c.close();}catch(Exception ignored){}if(slot)Main.connectionSlots.release();state.busy.set(false);}
  }
}
