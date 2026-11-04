import java.util.*;
import com.google.gson.*;
public final class RowEditorTest {
  public static void main(String[] args)throws Exception {
    var shape=new RowEditor.Shape("schema","table",List.of(),List.of("id"),"signature");
    var state=new RowEditor.State();var d=new RowEditor.Draft(shape,new JsonObject(),new JsonObject());state.drafts.put(d.id,d);
    if(RowEditor.draft(state,d.id)!=d)throw new AssertionError("Fresh draft missing");
    var field=RowEditor.Draft.class.getDeclaredField("created");field.setAccessible(true);field.setLong(d,System.nanoTime()-RowEditor.TTL-1);
    try{RowEditor.draft(state,d.id);throw new AssertionError("Expired draft allowed");}catch(IllegalArgumentException expected){}
    if(state.drafts.containsKey(d.id))throw new AssertionError("Expired draft retained");
    System.out.println("Row edit monotonic expiry: passed (no real-time wait)");
  }
}
