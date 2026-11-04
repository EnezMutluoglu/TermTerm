import com.google.gson.*;
import java.sql.*;
import java.util.*;
import java.math.BigDecimal;
import java.util.concurrent.atomic.AtomicBoolean;

/** Fixed single-row INSERT. Omitted columns use server defaults; values never become SQL. */
final class RowInsert {
  static final class State {final AtomicBoolean busy=new AtomicBoolean();volatile boolean closed;volatile Connection active;Draft draft;}
  record Draft(String id,long created,String hash,RowEditor.Shape shape,String objectId,JsonObject values,String sql){}
  static void close(Main.Session s){s.inserts.closed=true;s.inserts.draft=null;var c=s.inserts.active;if(c!=null)try{c.abort(Main.workers);}catch(Exception ignored){}}
  static String sql(RowEditor.Shape shape,String engine,JsonObject values){
    String target=RowEditor.target(shape,engine),head="INSERT INTO "+target;
    if(!values.isEmpty())head+=" ("+String.join(",",values.keySet().stream().map(k->DbTools.q(k,engine)).toList())+")";
    if(engine.equals("mssql"))head+=" OUTPUT "+String.join(",",shape.keys().stream().map(k->"INSERTED."+DbTools.q(k,engine)).toList());
    if(values.isEmpty()){
      if(engine.equals("oracle"))head+=" ("+DbTools.q(shape.keys().getFirst(),engine)+") VALUES (DEFAULT)";
      else head+=" DEFAULT VALUES";
    }else head+=" VALUES ("+String.join(",",Collections.nCopies(values.size(),"?"))+")";
    if(engine.equals("postgresql"))head+=" RETURNING "+String.join(",",shape.keys().stream().map(k->DbTools.q(k,engine)).toList());
    return head;
  }
  static JsonObject validate(RowEditor.Shape shape,String engine,JsonObject values){
    if(values==null)throw new IllegalArgumentException("Alan değerleri gerekli");values=values.deepCopy();int size=0;
    for(var entry:values.entrySet()){
      var col=RowEditor.column(shape,entry.getKey());if(RowEditor.generated(col))throw new IllegalArgumentException("Üretilen/identity alanı sunucuya bırakılmalı: "+entry.getKey());
      if(engine.equals("oracle")&&RowEditor.TEXT.contains(RowEditor.number(col,"data_type"))&&entry.getValue().isJsonPrimitive()&&entry.getValue().getAsString().isEmpty())entry.setValue(JsonNull.INSTANCE);
      RowEditor.value(col,entry.getValue());size+=entry.getValue().toString().length();
    }
    if(size>131072)throw new IllegalArgumentException("Satır 131.072 karakter sınırını aşıyor");
    for(var col:shape.columns())if(!values.has(col.get("column_name").toString())&&!RowEditor.generated(col)&&col.get("column_def")==null&&RowEditor.number(col,"nullable")==0)throw new IllegalArgumentException("Zorunlu alan için değer gerekli: "+col.get("column_name"));
    return values;
  }
  static JsonObject keys(ResultSet rs,RowEditor.Shape shape)throws Exception{
    if(!rs.next())throw new SQLException("Eklenen anahtar dönmedi");var keys=new JsonObject();
    for(int i=0;i<shape.keys().size();i++){String value=rs.getString(i+1);if(value==null)throw new SQLException("Eklenen anahtar NULL");keys.addProperty(shape.keys().get(i),value);}
    if(rs.next())throw new SQLException("Birden fazla satır döndü");return keys;
  }
  static void verify(RowEditor.Shape shape,JsonObject expected,JsonObject actual)throws Exception{
    if(actual==null)throw new SQLException("Eklenen satır geri okunamadı");
    for(var entry:expected.entrySet()){
      var col=RowEditor.column(shape,entry.getKey());Object a=RowEditor.value(col,entry.getValue()),b=RowEditor.value(col,actual.get(entry.getKey()));
      if(a instanceof java.sql.Date x&&!(a instanceof Timestamp))a=Timestamp.valueOf(x.toLocalDate().atStartOfDay());
      if(b instanceof java.sql.Date x&&!(b instanceof Timestamp))b=Timestamp.valueOf(x.toLocalDate().atStartOfDay());
      if(Set.of(Types.CHAR,Types.NCHAR).contains(RowEditor.number(col,"data_type"))){if(a instanceof String x)a=x.stripTrailing();if(b instanceof String x)b=x.stripTrailing();}
      if(!(a instanceof BigDecimal x&&b instanceof BigDecimal y?x.compareTo(y)==0:Objects.equals(a,b)))throw new SQLException("Sunucu değeri değiştirdi/hassasiyet kaybı: "+entry.getKey());
    }
  }
  static Object execute(Main.Session s,JsonObject p)throws Exception{
    String op=Main.str(p,"op","");var allowed=new HashSet<>(Set.of("id","op","sessionId"));
    allowed.addAll(op.equals("rowInsertMetadata")?Set.of("schema","table"):op.equals("rowInsertPreview")?Set.of("schema","table","values"):Set.of("planId","hash"));
    if(!allowed.containsAll(p.keySet()))throw new IllegalArgumentException("Beklenmeyen ekleme alanı");
    var state=s.inserts;if(!state.busy.compareAndSet(false,true))throw new IllegalStateException("Ekleme işlemi sürüyor");
    Connection c=null;boolean slot=false,write=false,commit=false;Draft d=null;
    try{
      if(state.closed)throw new IllegalStateException("Bağlantı kapandı");
      if(op.equals("rowInsertDiscard")){if(state.draft!=null&&state.draft.id.equals(Main.str(p,"planId","")))state.draft=null;return Map.of("discarded",true);}
      if(op.equals("rowInsertApply")){d=state.draft;if(d==null||ChangePlans.expired(d.created,System.nanoTime())||!d.id.equals(Main.str(p,"planId",""))||!d.hash.equals(Main.str(p,"hash","")))throw new IllegalArgumentException("Plan geçersiz/süresi doldu");state.draft=null;}else state.draft=null;
      if(!Main.connectionSlots.tryAcquire())throw new IllegalStateException("Bağlantı sınırı");slot=true;c=Main.connect(s.profile);state.active=c;c.setNetworkTimeout(Main.workers,20000);
      if(d!=null){c.setAutoCommit(false);c.setTransactionIsolation(Connection.TRANSACTION_READ_COMMITTED);try(var st=c.createStatement()){st.setQueryTimeout(10);String target=RowEditor.target(d.shape,s.engine);
        if(s.engine.equals("postgresql")){st.execute("SET LOCAL lock_timeout='5s'");st.execute("SET LOCAL statement_timeout='10s'");st.execute("LOCK TABLE "+target+" IN SHARE ROW EXCLUSIVE MODE");}
        else if(s.engine.equals("mssql")){st.execute("SET LOCK_TIMEOUT 5000");st.execute("SELECT TOP(1) 1 FROM "+target+" WITH (TABLOCKX,HOLDLOCK)");}
        else st.execute("LOCK TABLE "+target+" IN EXCLUSIVE MODE WAIT 5");}}
      String schema=d==null?Main.str(p,"schema",""):d.shape.schema(),table=d==null?Main.str(p,"table",""):d.shape.table();
      String objectId=RowDelete.guard(c,s.engine,schema,table,"INSERT");var shape=RowEditor.shape(c,s.engine,schema,table,DbTools.detail(c,s.engine,schema,table));
      if(op.equals("rowInsertMetadata")){var columns=new ArrayList<Object>();for(var col:shape.columns()){var item=new LinkedHashMap<String,Object>();item.put("name",col.get("column_name"));item.put("type",col.get("type_name"));item.put("generated",RowEditor.generated(col));item.put("nullable",RowEditor.number(col,"nullable")!=0);item.put("defaultValue",col.get("column_def"));item.put("primaryKey",shape.keys().contains(col.get("column_name")));columns.add(item);}return Map.of("columns",columns);}
      if(d==null){var values=validate(shape,s.engine,p.getAsJsonObject("values"));String sql=sql(shape,s.engine,values),hash=ChangePlans.hash(Main.JSON.toJson(List.of(objectId,shape.signature(),values,sql,s.profile.get("host"),s.profile.get("database"),c.getMetaData().getUserName())));
        d=new Draft(UUID.randomUUID().toString(),System.nanoTime(),hash,shape,objectId,values,sql);if(state.closed)throw new IllegalStateException("Bağlantı kapandı");state.draft=d;
        return Map.of("planId",d.id,"hash",hash,"sql",sql,"values",values,"expiresInSeconds",300,"connection",Map.of("host",Main.str(s.profile,"host",""),"database",Main.str(s.profile,"database",""),"username",c.getMetaData().getUserName()));}
      if(!objectId.equals(d.objectId)||!shape.signature().equals(d.shape.signature())){c.rollback();return Map.of("state","conflict","message","Tablo yapısı değişti; yeni önizleme gerekli.","rolledBack",true);}
      if(state.closed)throw new IllegalStateException("Bağlantı kapandı");JsonObject keys;
      try(var st=s.engine.equals("oracle")?c.prepareStatement(d.sql,shape.keys().toArray(String[]::new)):c.prepareStatement(d.sql)){
        st.setQueryTimeout(10);int i=1;for(var entry:d.values.entrySet())RowEditor.bind(st,i++,RowEditor.column(shape,entry.getKey()),entry.getValue());write=true;
        if(s.engine.equals("oracle")){if(st.executeUpdate()!=1)throw new SQLException("Tam bir satır eklenmedi");try(var rs=st.getGeneratedKeys()){keys=keys(rs,shape);}}
        else try(var rs=st.executeQuery()){keys=keys(rs,shape);}
      }
      var row=RowEditor.read(c,s.engine,shape,keys,false);verify(shape,d.values,row);if(state.closed)throw new IllegalStateException("Bağlantı kapandı");commit=true;c.commit();return Map.of("state","succeeded","message","1 satır eklendi ve commit edildi.","affectedRows",1,"keys",keys,"row",row);
    }catch(Exception e){if(!op.equals("rowInsertApply"))throw e;boolean rolled=false;if(c!=null)try{c.rollback();rolled=true;}catch(Exception ignored){}boolean unknown=commit||(write&&!rolled);return Map.of("state",unknown?"unknown_outcome":"failed","rolledBack",rolled&&!unknown,"message",unknown?"Sonuç doğrulanamadı; yeni bağlantıdan kontrol edin. Otomatik tekrar yok.":Objects.toString(e.getMessage(),"Ekleme başarısız"));}
    finally{state.active=null;if(c!=null)try{c.close();}catch(Exception ignored){}if(slot)Main.connectionSlots.release();state.busy.set(false);}
  }
}
