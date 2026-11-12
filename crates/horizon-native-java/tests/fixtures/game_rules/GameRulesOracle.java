import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.gamerules.GameRule;
import net.minecraft.world.level.gamerules.GameRules;

/** Prints, per GameRules static GameRule field in declaration order:
 *  FIELD id descriptionId type defaultValue(class:value) serialize(default) commandResult(default) featureGated
 *  then a vanilla-features GameRules' getAsString for each, and the exception for a rule it lacks. */
public class GameRulesOracle {
  @SuppressWarnings({"unchecked", "rawtypes"})
  public static void main(String[] a) throws Exception {
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    GameRules rules = new GameRules(FeatureFlags.VANILLA_SET);
    for (Field f : GameRules.class.getDeclaredFields()) {
      if (!Modifier.isStatic(f.getModifiers()) || f.getType() != GameRule.class) continue;
      GameRule r = (GameRule) f.get(null);
      Object d = r.defaultValue();
      String present;
      try { present = rules.getAsString(r); }
      catch (Exception e) { present = "THROWS " + e.getClass().getName() + ": " + e.getMessage(); }
      System.out.println(String.join("\t", f.getName(), r.id(), String.valueOf(r.getIdentifier()), r.toString(),
          r.getDescriptionId(), String.valueOf(r.gameRuleType()), d.getClass().getName() + ":" + d,
          r.serialize(d), String.valueOf(r.getCommandResult(d)), String.valueOf(!r.requiredFeatures().isEmpty()),
          r.valueClass().getName(), present));
    }
    // serialize/getCommandResult of non-default values
    System.out.println("serialize-false\t" + GameRules.KEEP_INVENTORY.serialize(false) + "\t" + GameRules.KEEP_INVENTORY.getCommandResult(false));
    System.out.println("serialize-int\t" + GameRules.RANDOM_TICK_SPEED.serialize(-7) + "\t" + GameRules.RANDOM_TICK_SPEED.getCommandResult(-7));
    try { rules.get(GameRules.MAX_MINECART_SPEED); System.out.println("get-gated\tno throw"); }
    catch (Exception e) { System.out.println("get-gated\t" + e.getClass().getName() + "\t" + e.getMessage()); }
    rules.set(GameRules.KEEP_INVENTORY, true, null);
    rules.set(GameRules.RANDOM_TICK_SPEED, 7, null);
    Object k = rules.get(GameRules.KEEP_INVENTORY), t = rules.get(GameRules.RANDOM_TICK_SPEED);
    System.out.println("after-set\t" + k.getClass().getName() + ":" + k + "\t" + t.getClass().getName() + ":" + t
        + "\t" + rules.getAsString(GameRules.KEEP_INVENTORY) + "\t" + rules.getAsString(GameRules.RANDOM_TICK_SPEED));
  }
}
