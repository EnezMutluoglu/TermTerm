import com.google.gson.*;
import java.sql.*;
import java.util.*;
import java.util.concurrent.*;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;

/** Explicit, bounded metadata capture on a connection independent of the editor. */
final class SchemaSnapshot {
  static final int MAX_OBJECTS=200, MAX_BYTES=4_000_000;
  static Object capture(Main.Session session,JsonObject request)throws Exception {
    String schema=Main.str(request,"schema","");
    DbTools.q(schema,session.engine);
    if(!session.snapshotBusy.compareAndSet(false,true))throw new IllegalStateException("Bu sekmede şema görüntüsü alınıyor");
    boolean slot=false;
    try {
      if(!(slot=Main.connectionSlots.tryAcquire()))throw new IllegalStateException("Bağlantı sınırına ulaşıldı");
      try(Connection c=Main.connect(session.profile)) {
        c.setNetworkTimeout(Main.workers,15000);
        var timer=Executors.newSingleThreadScheduledExecutor();
        var deadline=timer.schedule(()->{try{c.abort(Main.workers);}catch(SQLException ignored){}},30,TimeUnit.SECONDS);
        long start=System.currentTimeMillis();
        try {
          var md=c.getMetaData();
          boolean found=false;
          try(ResultSet rs=md.getSchemas()){while(rs.next())if(schema.equals(rs.getString("TABLE_SCHEM"))){found=true;break;}}
          if(!found)throw new SQLException("Şema görünmüyor veya mevcut değil", "42501");
          var names=new TreeMap<String,String>();
          try(ResultSet rs=md.getTables(c.getCatalog(),DbTools.pattern(md,schema),"%",new String[]{"TABLE","VIEW"})) {
            while(rs.next()) {
              if(!schema.equals(rs.getString("TABLE_SCHEM")))continue;
              names.put(rs.getString("TABLE_NAME"),rs.getString("TABLE_TYPE"));
              if(names.size()>MAX_OBJECTS)throw new SQLException("Şema 200 nesne sınırını aşıyor; görüntü kaydedilmedi", "54000");
            }
          }
          var objects=new ArrayList<Object>();int bytes=0;
          for(var entry:names.entrySet()) {
            if(System.currentTimeMillis()-start>30000)throw new SQLTimeoutException("Şema görüntüsü 30 saniye sınırını aştı");
            var detail=DbTools.detail(c,session.engine,schema,entry.getKey());
            var object=Map.of("name",entry.getKey(),"type",entry.getValue(),"detail",detail);
            bytes+=Main.JSON.toJson(object).getBytes(StandardCharsets.UTF_8).length;
            if(bytes>MAX_BYTES)throw new SQLException("Şema görüntüsü 4 MB sınırını aşıyor; kaydedilmedi", "54000");
            objects.add(object);
          }
          // Scope intentionally excludes passwords, certificates and transient tunnel ports.
          var scope=new LinkedHashMap<String,Object>();
          for(String key:List.of("engine","host","port","database","username","sshHostId"))scope.put(key,session.profile.get(key));
          scope.put("catalog",c.getCatalog());scope.put("schema",schema);scope.put("principal",md.getUserName());
          String source=HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(Main.JSON.toJson(scope).getBytes(StandardCharsets.UTF_8)));
          return Map.of("format",1,"source",source,"schema",schema,"engine",session.engine,
              "username",md.getUserName(),"version",md.getDatabaseProductVersion(),"capturedAt",System.currentTimeMillis(),
              "objects",objects,"coverage","visible_metadata","durationMs",System.currentTimeMillis()-start);
        } finally {deadline.cancel(false);timer.shutdownNow();}
      }
    } finally {if(slot)Main.connectionSlots.release();session.snapshotBusy.set(false);}
  }
}
