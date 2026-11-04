import com.google.gson.*;
import java.sql.*;
import java.util.*;

/** Exact metadata selectors; patterns supplied by users are never passed to JDBC. */
final class MetadataOptions {
  static Object read(Connection c,String engine,JsonObject p)throws SQLException {
    var md=c.getMetaData();String kind=Main.str(p,"kind","schemas"),schema=Main.str(p,"schema",""),table=Main.str(p,"table","");
    if(kind.equals("schemas"))return Map.of("items",DbTools.objects(md.getSchemas()).stream().map(r->r.get("table_schem")).filter(Objects::nonNull).distinct().sorted(Comparator.comparing(Object::toString)).toList());
    DbTools.q(schema,engine);
    if(kind.equals("tables"))return Map.of("items",DbTools.objects(md.getTables(c.getCatalog(),DbTools.pattern(md,schema),"%",new String[]{"TABLE","VIEW"})).stream().filter(r->schema.equals(r.get("table_schem"))).map(r->r.get("table_name")).distinct().sorted(Comparator.comparing(Object::toString)).toList());
    if(!kind.equals("columns"))throw new IllegalArgumentException("Geçersiz metadata seçimi");
    DbTools.q(table,engine);
    var rows=DbTools.objects(md.getColumns(c.getCatalog(),DbTools.pattern(md,schema),DbTools.pattern(md,table),"%"));
    return Map.of("items",rows.stream().filter(r->schema.equals(r.get("table_schem"))&&table.equals(r.get("table_name"))).map(r->r.get("column_name")).distinct().toList());
  }
}
