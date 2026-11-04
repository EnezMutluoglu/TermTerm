import com.google.gson.*;
import java.sql.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.*;

/** Explicit bounded full-value read. A single SELECT supplies the complete value. */
final class LobValue {
  static final java.util.concurrent.Semaphore slots=new java.util.concurrent.Semaphore(2);
  static final int LIMIT=8*1024*1024;
  static final Set<Integer> BINARY=Set.of(Types.BLOB,Types.BINARY,Types.VARBINARY,Types.LONGVARBINARY);
  static final Set<Integer> TEXT=Set.of(Types.CLOB,Types.NCLOB,Types.CHAR,Types.NCHAR,Types.VARCHAR,Types.NVARCHAR,Types.LONGVARCHAR,Types.LONGNVARCHAR);
  static List<String> keys(Map<String,Object> detail){
    var columns=(List<Map<String,Object>>)detail.get("columns");var pk=new ArrayList<>((List<Map<String,Object>>)detail.get("primaryKeys"));pk.sort(Comparator.comparingInt(c->RowEditor.number(c,"key_seq")));
    var keys=pk.stream().map(c->c.get("column_name").toString()).toList();
    if(keys.isEmpty())throw new IllegalArgumentException("Tam değer için birincil anahtar gerekli");
    for(String k:keys){var c=columns.stream().filter(v->k.equals(v.get("column_name"))).findFirst().orElseThrow();int type=RowEditor.number(c,"data_type"),size=RowEditor.number(c,"column_size");
      if(RowEditor.number(c,"nullable")!=0||(!RowEditor.NUMERIC.contains(type)&&!RowEditor.TEXT.contains(type)&&!RowEditor.OTHER.contains(type))||(RowEditor.TEXT.contains(type)&&(size<=0||size>2048))||(RowEditor.NUMERIC.contains(type)&&size>1000))throw new IllegalArgumentException("Anahtar türü veya uzunluğu güvenilir satır seçimine uygun değil");
    }return keys;
  }
  static Object support(Map<String,Object> detail){
    try{return Map.of("keyColumns",keys(detail),"columns",((List<Map<String,Object>>)detail.get("columns")).stream().filter(c->BINARY.contains(RowEditor.number(c,"data_type"))||TEXT.contains(RowEditor.number(c,"data_type"))).map(c->c.get("column_name")).toList());}
    catch(Exception e){return Map.of("keyColumns",List.of(),"columns",List.of(),"reason",e.getMessage());}
  }
  static final class Bounded extends ByteArrayOutputStream {
    @Override public synchronized void write(byte[] b,int off,int len){if(count+len>LIMIT)throw new IllegalArgumentException("Değer 8 MiB sınırını aşıyor; kısmi içerik döndürülmedi");super.write(b,off,len);}
    @Override public synchronized void write(int b){if(count>=LIMIT)throw new IllegalArgumentException("Değer 8 MiB sınırını aşıyor; kısmi içerik döndürülmedi");super.write(b);}
  }
  static Object read(Connection c,String engine,JsonObject p)throws Exception {
    if(!slots.tryAcquire())throw new IllegalStateException("İki büyük değer okunuyor; bitince tekrar deneyin");
    try{return fetch(c,engine,p);}finally{slots.release();}
  }
  static void deadline(long start){if(System.nanoTime()-start>15_000_000_000L)throw new IllegalStateException("Tam değer okuma süresi doldu; kısmi içerik döndürülmedi");}
  static Object fetch(Connection c,String engine,JsonObject p)throws Exception {
    long start=System.nanoTime();
    String schema=Main.str(p,"schema",""),table=Main.str(p,"table",""),name=Main.str(p,"column","");DbTools.q(schema,engine);DbTools.q(table,engine);
    var md=c.getMetaData();boolean base=false;try(var rs=md.getTables(c.getCatalog(),DbTools.pattern(md,schema),DbTools.pattern(md,table),new String[]{"TABLE"})){while(rs.next())if(schema.equals(rs.getString("TABLE_SCHEM"))&&table.equals(rs.getString("TABLE_NAME")))base=true;}
    if(!base)throw new IllegalArgumentException("Tam değer yalnız anahtarlı gerçek tablodan okunur");
    var detail=DbTools.detail(c,engine,schema,table);var keys=keys(detail);var columns=(List<Map<String,Object>>)detail.get("columns");
    var col=columns.stream().filter(v->name.equals(v.get("column_name"))).findFirst().orElseThrow(()->new IllegalArgumentException("Sütun görünmüyor"));int type=RowEditor.number(col,"data_type");boolean binary=BINARY.contains(type);
    if(!binary&&!TEXT.contains(type))throw new IllegalArgumentException("Bu tür için tam değer okuyucu yok");
    JsonObject values=p.getAsJsonObject("keys");if(values==null||!values.keySet().equals(new HashSet<>(keys)))throw new IllegalArgumentException("Anahtar alanları eşleşmiyor");
    String selected=DbTools.q(name,engine);
    // PostgreSQL bytea/JDBC can buffer the value before exposing a stream; bound it on the server too.
    if(engine.equals("postgresql"))selected="substring("+selected+" from 1 for "+(LIMIT+1)+")";
    else if(engine.equals("mssql")&&!"uniqueidentifier".equalsIgnoreCase(col.get("type_name").toString()))selected="SUBSTRING("+selected+",1,"+(LIMIT+1)+")";
    String sql="SELECT "+selected+" FROM "+DbTools.q(schema,engine)+"."+DbTools.q(table,engine)+" WHERE "+String.join(" AND ",keys.stream().map(k->DbTools.q(k,engine)+"=?").toList());
    try(PreparedStatement st=c.prepareStatement(sql)){
      st.setQueryTimeout(15);st.setMaxRows(2);int i=1;for(String key:keys)RowEditor.bind(st,i++,columns.stream().filter(v->key.equals(v.get("column_name"))).findFirst().orElseThrow(),values.get(key));
      try(ResultSet rs=st.executeQuery()){
        if(!rs.next())throw new IllegalArgumentException("Satır silindi veya artık görünmüyor");var out=new Bounded();boolean isNull;
        if(binary){try(InputStream in=rs.getBinaryStream(1)){isNull=in==null;if(in!=null){byte[] chunk=new byte[8192];int n;while((n=in.read(chunk))!=-1){deadline(start);out.write(chunk,0,n);}}}}
        else if("uniqueidentifier".equalsIgnoreCase(col.get("type_name").toString())){String value=rs.getString(1);isNull=value==null;if(!isNull)out.write(value.getBytes(StandardCharsets.UTF_8));}
        else {try(Reader in=rs.getCharacterStream(1)){isNull=in==null;if(in!=null){try(Writer writer=new OutputStreamWriter(out,StandardCharsets.UTF_8.newEncoder())){char[] chunk=new char[8192];int n;while((n=in.read(chunk))!=-1){deadline(start);writer.write(chunk,0,n);}}}}}
        if(rs.next())throw new IllegalArgumentException("Anahtar birden fazla satır seçti; içerik döndürülmedi");
        byte[] bytes=out.toByteArray();return Map.of("base64",Base64.getEncoder().encodeToString(bytes),"bytes",bytes.length,"sha256",HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes)),"binary",binary,"isNull",isNull,"encoding",binary?"binary":"UTF-8","observedAt",System.currentTimeMillis());
      }
    }
  }
}
