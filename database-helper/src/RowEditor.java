import com.google.gson.*;
import java.sql.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;
import java.math.*;

/** Session-bound, one-use edit plans. No SQL supplied by the renderer is executed here. */
final class RowEditor {
  static final long TTL=300_000_000_000L;
  static final Set<Integer> NUMERIC=Set.of(Types.NUMERIC,Types.DECIMAL,Types.BIGINT,Types.INTEGER,Types.SMALLINT,Types.TINYINT);
  static final Set<Integer> TEXT=Set.of(Types.VARCHAR,Types.NVARCHAR,Types.CHAR,Types.NCHAR);
  static final Set<Integer> OTHER=Set.of(Types.DATE,Types.TIME,Types.TIMESTAMP,Types.BOOLEAN,Types.BIT);
  static final class State {
    final Map<String,Draft> drafts=new HashMap<>();final AtomicBoolean busy=new AtomicBoolean();volatile boolean closed=false;volatile Connection active;
  }
  record Shape(String schema,String table,List<Map<String,Object>> columns,List<String> keys,String signature) {}
  static final class Draft {
    final String id=UUID.randomUUID().toString();final long created=System.nanoTime();final Shape shape;final JsonObject keys,original;
    JsonObject changes;String planId;
    Draft(Shape shape,JsonObject keys,JsonObject original){this.shape=shape;this.keys=keys.deepCopy();this.original=original.deepCopy();}
  }
  static boolean yes(Object value){return Set.of("YES","TRUE","1").contains(Objects.toString(value,"").toUpperCase(Locale.ROOT));}
  static int number(Map<String,Object> col,String name){try{return Integer.parseInt(Objects.toString(col.get(name),"0"));}catch(Exception e){return 0;}}
  static boolean generated(Map<String,Object> col){return yes(col.get("is_autoincrement"))||yes(col.get("is_generatedcolumn"))||yes(col.get("ss_is_computed"));}
  static Shape shape(Connection c,String engine,String schema,String table,Map<String,Object> detail)throws SQLException {
    var md=c.getMetaData();boolean base=false;
    try(ResultSet rs=md.getTables(c.getCatalog(),DbTools.pattern(md,schema),DbTools.pattern(md,table),new String[]{"TABLE"})){
      while(rs.next())if(schema.equals(rs.getString("TABLE_SCHEM"))&&table.equals(rs.getString("TABLE_NAME")))base=true;
    }
    if(!base)throw new IllegalArgumentException("Yalnız gerçek tek tablo düzenlenebilir; görünüm salt okunur.");
    if(engine.equals("postgresql"))try(PreparedStatement st=c.prepareStatement("SELECT c.relkind,EXISTS(SELECT 1 FROM pg_inherits i WHERE i.inhrelid=c.oid OR i.inhparent=c.oid) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=? AND c.relname=?")){
      st.setQueryTimeout(5);st.setString(1,schema);st.setString(2,table);try(ResultSet rs=st.executeQuery()){if(!rs.next()||!"r".equals(rs.getString(1))||rs.getBoolean(2))throw new IllegalArgumentException("Partition/inheritance tablosu bu aşamada salt okunur.");}
    }
    var columns=(List<Map<String,Object>>)detail.get("columns");
    if(columns.size()>100)throw new IllegalArgumentException("100 sütundan geniş tablolar bu aşamada salt okunur.");
    for(var col:columns){int type=number(col,"data_type");if(TEXT.contains(type)&&(number(col,"column_size")<=0||number(col,"column_size")>16384))throw new IllegalArgumentException("Sınırsız veya büyük metin alanı bu aşamada salt okunur: "+col.get("column_name"));if(NUMERIC.contains(type)&&number(col,"column_size")>1000)throw new IllegalArgumentException("Sınırsız sayısal alan bu aşamada salt okunur: "+col.get("column_name"));if(!NUMERIC.contains(type)&&!TEXT.contains(type)&&!OTHER.contains(type))throw new IllegalArgumentException("Desteklenmeyen düzenleme türü: "+col.get("column_name")+" ("+col.get("type_name")+"). LOB/binary/JSON/floating alanlı tablolar salt okunur.");}
    var pk=new ArrayList<>((List<Map<String,Object>>)detail.get("primaryKeys"));pk.sort(Comparator.comparingInt(v->number(v,"key_seq")));
    var keys=pk.stream().map(v->Objects.toString(v.get("column_name"))).toList();
    if(keys.isEmpty()||keys.stream().anyMatch(k->columns.stream().noneMatch(v->k.equals(v.get("column_name"))&&number(v,"nullable")==0)))throw new IllegalArgumentException("Düzenleme için NULL kabul etmeyen birincil anahtar gerekli.");
    for(String key:keys){var col=columns.stream().filter(v->key.equals(v.get("column_name"))).findFirst().orElseThrow();if(TEXT.contains(number(col,"data_type"))&&number(col,"column_size")>2048)throw new IllegalArgumentException("Uzun metin anahtarı gezginde kısaltılabileceğinden düzenleme kapalı.");}
    // Generated flags are driver-dependent; read the native metadata as well.
    Set<String> generated=new HashSet<>();
    String sql=engine.equals("oracle")?"SELECT column_name FROM all_tab_cols WHERE owner=? AND table_name=? AND (virtual_column='YES' OR identity_column='YES')":engine.equals("mssql")?"SELECT c.name FROM sys.columns c JOIN sys.objects t ON t.object_id=c.object_id JOIN sys.schemas s ON s.schema_id=t.schema_id WHERE s.name=? AND t.name=? AND (c.is_computed=1 OR c.is_identity=1 OR c.generated_always_type<>0)":"SELECT a.attname FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=? AND c.relname=? AND (a.attgenerated<>'' OR a.attidentity<>'')";
    try(PreparedStatement st=c.prepareStatement(sql)){st.setQueryTimeout(5);st.setString(1,schema);st.setString(2,table);try(ResultSet rs=st.executeQuery()){while(rs.next())generated.add(rs.getString(1));}}
    var copy=new ArrayList<Map<String,Object>>();
    for(var col:columns){var v=new LinkedHashMap<>(col);if(generated.contains(v.get("column_name")))v.put("is_generatedcolumn","YES");copy.add(v);}
    var signature=Main.JSON.toJson(Map.of("columns",copy,"keys",keys));
    return new Shape(schema,table,copy,keys,signature);
  }
  static Object support(Connection c,String engine,String schema,String table,Map<String,Object> detail){
    try{var shape=shape(c,engine,schema,table,detail);return Map.of("enabled",true,"keyColumns",shape.keys);}
    catch(Exception e){return Map.of("enabled",false,"reason",Objects.toString(e.getMessage(),"Düzenleme doğrulanamadı"),"keyColumns",List.of());}
  }
  static Map<String,Object> column(Shape shape,String name){return shape.columns.stream().filter(c->name.equals(c.get("column_name"))).findFirst().orElseThrow(()->new IllegalArgumentException("Sütun bulunamadı"));}
  static Object value(Map<String,Object> col,JsonElement value) {
    if(value==null||value.isJsonNull()){if(number(col,"nullable")==0)throw new IllegalArgumentException(col.get("column_name")+" NULL kabul etmiyor");return null;}
    if(!value.isJsonPrimitive()||!value.getAsJsonPrimitive().isString())throw new IllegalArgumentException("Değerler metin veya NULL olmalı; sayılar hassasiyet kaybetmeden aktarılır.");
    String text=value.getAsString();if(text.length()>16384)throw new IllegalArgumentException("Alan 16.384 karakter sınırını aşıyor");int type=number(col,"data_type");
    if(NUMERIC.contains(type)){
      BigDecimal n=new BigDecimal(text);if(Math.abs((long)n.scale())>1000||n.precision()>1000)throw new IllegalArgumentException("Sayı sınırı aşıldı");
      int precision=number(col,"column_size"),scale=number(col,"decimal_digits");
      if(precision>0&&precision<=1000){if(scale!=-127)n=n.setScale(scale,RoundingMode.UNNECESSARY);if(n.precision()>precision)throw new IllegalArgumentException("Sayı sütun hassasiyetini aşıyor");}
      return n;
    }
    if(type==Types.BOOLEAN||type==Types.BIT){if(Set.of("true","t","1").contains(text.toLowerCase(Locale.ROOT)))return true;if(Set.of("false","f","0").contains(text.toLowerCase(Locale.ROOT)))return false;throw new IllegalArgumentException("Boolean alan true/false olmalı");}
    if(type==Types.TIMESTAMP)return text.length()==10?Timestamp.valueOf(java.time.LocalDate.parse(text).atStartOfDay()):Timestamp.valueOf(text.replace('T',' '));
    if(type==Types.DATE)return text.length()>10?Timestamp.valueOf(text.replace('T',' ')):java.sql.Date.valueOf(text);
    if(type==Types.TIME)return java.sql.Time.valueOf(text);
    int size=number(col,"column_size");if(size>0&&text.codePointCount(0,text.length())>size)throw new IllegalArgumentException("Metin sütun uzunluğunu aşıyor");return text;
  }
  static void bind(PreparedStatement st,int index,Map<String,Object> col,JsonElement v)throws SQLException {
    Object x=value(col,v);if(x==null)st.setNull(index,number(col,"data_type"));else if(x instanceof BigDecimal n)st.setBigDecimal(index,n);else if(x instanceof Timestamp t)st.setTimestamp(index,t);else if(x instanceof java.sql.Date d)st.setDate(index,d);else if(x instanceof java.sql.Time t)st.setTime(index,t);else if(x instanceof Boolean b)st.setBoolean(index,b);else if(Set.of(Types.NVARCHAR,Types.NCHAR).contains(number(col,"data_type")))st.setNString(index,x.toString());else st.setString(index,x.toString());
  }
  static String target(Shape s,String engine){return DbTools.q(s.schema,engine)+"."+DbTools.q(s.table,engine);}
  static String where(Shape s,String engine){return String.join(" AND ",s.keys.stream().map(k->DbTools.q(k,engine)+"=?").toList());}
  static JsonObject read(Connection c,String engine,Shape shape,JsonObject keys,boolean lock)throws SQLException {
    String sql="SELECT "+String.join(",",shape.columns.stream().map(v->DbTools.q(v.get("column_name").toString(),engine)).toList())+" FROM "+target(shape,engine)+(lock&&engine.equals("mssql")?" WITH (UPDLOCK,HOLDLOCK,ROWLOCK)":"")+" WHERE "+where(shape,engine)+(lock&&!engine.equals("mssql")?" FOR UPDATE"+(engine.equals("oracle")?" WAIT 5":""):"");
    try(PreparedStatement st=c.prepareStatement(sql)){st.setQueryTimeout(10);int i=1;for(String key:shape.keys)bind(st,i++,column(shape,key),keys.get(key));
      try(ResultSet rs=st.executeQuery()){
        if(!rs.next())return null;JsonObject row=new JsonObject();int size=0;
        for(i=0;i<shape.columns.size();i++){String text=rs.getString(i+1);if(text!=null){size+=text.length();if(text.length()>16384||size>131072)throw new IllegalArgumentException("Satır düzenleme boyut sınırını aşıyor; değerler kısaltılmadı");}row.addProperty(shape.columns.get(i).get("column_name").toString(),text);}
        if(rs.next())throw new IllegalArgumentException("Anahtar birden fazla satır seçti; işlem reddedildi");return row;
      }
    }
  }
  static void close(Main.Session session){State state=session.edits;state.closed=true;synchronized(state){state.drafts.clear();}Connection c=state.active;if(c!=null)try{c.abort(Main.workers);}catch(Exception ignored){}}
  static Draft draft(State state,String id){Draft d=state.drafts.get(id);if(d==null||System.nanoTime()-d.created>TTL){state.drafts.remove(id);throw new IllegalArgumentException("Taslak sona erdi veya başka oturuma ait; satırı yeniden açın");}return d;}
  static Object execute(Main.Session session,JsonObject p)throws Exception {
    State state=session.edits;String op=Main.str(p,"op","");
    if(op.equals("rowEditDiscard")){synchronized(state){state.drafts.remove(Main.str(p,"draftId",""));}return Map.of("discarded",true);}
    if(!state.busy.compareAndSet(false,true))throw new IllegalStateException("Satır düzenleme işlemi sürüyor");
    try {
      if(state.closed)throw new IllegalStateException("Bağlantı kapandı");
      if(op.equals("rowEditPreview"))synchronized(state){
        Draft d=draft(state,Main.str(p,"draftId",""));d.planId=null;d.changes=null;
        JsonObject changes=p.has("changes")?p.getAsJsonObject("changes").deepCopy():null;if(changes==null||changes.isEmpty()||changes.size()>100)throw new IllegalArgumentException("Değiştirilecek alan seçin");
        for(var entry:changes.entrySet()){var col=column(d.shape,entry.getKey());if(d.shape.keys.contains(entry.getKey())||generated(col))throw new IllegalArgumentException("Anahtar/üretilen sütun değiştirilemez");if(session.engine.equals("oracle")&&TEXT.contains(number(col,"data_type"))&&entry.getValue().isJsonPrimitive()&&entry.getValue().getAsString().isEmpty())changes.add(entry.getKey(),JsonNull.INSTANCE);value(col,changes.get(entry.getKey()));}
        d.changes=changes.deepCopy();d.planId=UUID.randomUUID().toString();
        var fields=new ArrayList<Object>();for(var e:changes.entrySet()){var v=new LinkedHashMap<String,Object>();v.put("column",e.getKey());v.put("before",d.original.get(e.getKey()));v.put("after",e.getValue());fields.add(v);}
        return Map.of("planId",d.planId,"draftId",d.id,"schema",d.shape.schema,"table",d.shape.table,"changes",fields,"sql",update(d,session.engine),"keys",d.keys,"expiresInSeconds",Math.max(0,(TTL-(System.nanoTime()-d.created))/1_000_000_000L));
      }
      if(op.equals("rowEditApply")){
        Draft d;synchronized(state){d=draft(state,Main.str(p,"draftId",""));if(d.planId==null||!d.planId.equals(Main.str(p,"planId","")))throw new IllegalArgumentException("Önizleme değişti; güncel planı onaylayın");state.drafts.remove(d.id);}
        return apply(session,d);
      }
      if(!op.equals("rowEditOpen"))throw new IllegalArgumentException("Geçersiz satır işlemi");
      synchronized(state){state.drafts.values().removeIf(d->System.nanoTime()-d.created>TTL);if(state.drafts.size()>=4)throw new IllegalStateException("Bu sekmede en fazla 4 düzenleme taslağı; açık taslakları kapatın");}
      if(!Main.connectionSlots.tryAcquire())throw new IllegalStateException("Bağlantı sınırı");
      try(Connection c=Main.connect(session.profile)){
        state.active=c;c.setNetworkTimeout(Main.workers,15000);if(state.closed)throw new IllegalStateException("Bağlantı kapandı");
        String schema=Main.str(p,"schema",""),table=Main.str(p,"table","");Shape shape=shape(c,session.engine,schema,table,DbTools.detail(c,session.engine,schema,table));
        JsonObject keys=p.getAsJsonObject("keys");if(keys==null||!keys.keySet().equals(new HashSet<>(shape.keys)))throw new IllegalArgumentException("Birincil anahtar alanları eşleşmiyor");
        JsonObject original=read(c,session.engine,shape,keys,false);if(original==null)throw new IllegalArgumentException("Satır artık görünmüyor; tabloyu yenileyin");
        Draft d=new Draft(shape,keys,original);synchronized(state){if(state.closed)throw new IllegalStateException("Bağlantı kapandı");state.drafts.put(d.id,d);}
        var columns=new ArrayList<Object>();for(var col:shape.columns){String name=col.get("column_name").toString();columns.add(Map.of("name",name,"type",col.get("type_name"),"nullable",number(col,"nullable")!=0,"readOnly",shape.keys.contains(name)||generated(col)));}
        return Map.of("draftId",d.id,"schema",schema,"table",table,"original",original,"columns",columns,"expiresInSeconds",300);
      }finally{state.active=null;Main.connectionSlots.release();}
    }finally{state.busy.set(false);}
  }
  static String update(Draft d,String engine){return "UPDATE "+target(d.shape,engine)+" SET "+String.join(",",d.changes.keySet().stream().map(k->DbTools.q(k,engine)+"=?").toList())+" WHERE "+where(d.shape,engine);}
  static Object apply(Main.Session session,Draft d)throws Exception {
    if(!Main.connectionSlots.tryAcquire())throw new IllegalStateException("Bağlantı sınırı; yeni önizleme açın");
    Connection c=null;boolean writeStarted=false,commitStarted=false;
    try{
      c=Main.connect(session.profile);session.edits.active=c;c.setNetworkTimeout(Main.workers,15000);c.setTransactionIsolation(Connection.TRANSACTION_READ_COMMITTED);c.setAutoCommit(false);
      if(session.edits.closed)throw new IllegalStateException("Bağlantı kapandı");
      if(session.engine.equals("mssql")||session.engine.equals("postgresql"))try(Statement st=c.createStatement()){st.setQueryTimeout(5);st.execute(session.engine.equals("mssql")?"SET LOCK_TIMEOUT 5000":"SET LOCAL lock_timeout='5s'");}
      Shape current=shape(c,session.engine,d.shape.schema,d.shape.table,DbTools.detail(c,session.engine,d.shape.schema,d.shape.table));
      if(!current.signature.equals(d.shape.signature)){c.rollback();return Map.of("state","conflict","message","Tablo yapısı değişti; yeni taslak açın","rolledBack",true);}
      JsonObject row=read(c,session.engine,current,d.keys,true);
      if(!d.original.equals(row)){c.rollback();var r=new LinkedHashMap<String,Object>();r.put("state","conflict");r.put("message",row==null?"Satır silindi veya artık görünmüyor":"Satır başka bir işlem tarafından değiştirildi; üzerine yazılmadı");r.put("current",row);r.put("rolledBack",true);return r;}
      if(session.edits.closed)throw new IllegalStateException("Bağlantı kapandı");
      try(PreparedStatement st=c.prepareStatement(update(d,session.engine))){st.setQueryTimeout(10);int i=1;for(var e:d.changes.entrySet())bind(st,i++,column(current,e.getKey()),e.getValue());for(String key:current.keys)bind(st,i++,column(current,key),d.keys.get(key));writeStarted=true;if(st.executeUpdate()!=1)throw new SQLException("Tam olarak bir satır güncellenmedi; işlem geri alındı");}
      JsonObject updated=read(c,session.engine,current,d.keys,false);if(updated==null)throw new SQLException("Güncellenen satır doğrulanamadı; işlem geri alındı");
      for(var e:d.changes.entrySet()){
        var col=column(current,e.getKey());Object expected=value(col,e.getValue()),actual=value(col,updated.get(e.getKey()));
        if(expected instanceof java.sql.Date date && !(expected instanceof Timestamp))expected=Timestamp.valueOf(date.toLocalDate().atStartOfDay());
        if(actual instanceof java.sql.Date date && !(actual instanceof Timestamp))actual=Timestamp.valueOf(date.toLocalDate().atStartOfDay());
        if(expected instanceof String x&&Set.of(Types.CHAR,Types.NCHAR).contains(number(col,"data_type")))expected=x.replaceFirst(" +$","");
        if(actual instanceof String x&&Set.of(Types.CHAR,Types.NCHAR).contains(number(col,"data_type")))actual=x.replaceFirst(" +$","");
        boolean equal=expected instanceof BigDecimal a&&actual instanceof BigDecimal b?a.compareTo(b)==0:Objects.equals(expected,actual);
        if(!equal)throw new SQLException("Sunucu değeri değiştirdi veya hassasiyet kaybı oluştu ("+e.getKey()+"); işlem geri alındı");
      }
      if(session.edits.closed)throw new IllegalStateException("Bağlantı kapandı");
      commitStarted=true;c.commit();return Map.of("state","succeeded","affectedRows",1,"committed",true,"row",updated);
    }catch(Exception e){
      boolean rolledBack=false;if(c!=null)try{c.rollback();rolledBack=true;}catch(Exception ignored){}
      boolean unknown=commitStarted||(writeStarted&&!rolledBack);
      return Map.of("state",unknown?"unknown_outcome":"failed","rolledBack",!unknown&&rolledBack,"message",unknown?"Kaydetme sonucu doğrulanamadı. Otomatik tekrar yapılmadı; yeni bağlantıdan satırı kontrol edin.":Objects.toString(e.getMessage(),"Satır kaydedilemedi"));
    }finally{session.edits.active=null;if(c!=null)try{c.close();}catch(Exception ignored){}Main.connectionSlots.release();}
  }
}
