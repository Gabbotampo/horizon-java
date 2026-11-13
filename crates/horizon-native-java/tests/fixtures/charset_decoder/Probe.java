package demo;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.nio.charset.CharsetDecoder;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;

public final class Probe {
    static String attempt(Runnable body) {
        try {
            body.run();
            return "ok";
        } catch (IllegalArgumentException error) {
            return "iae:" + error.getMessage();
        }
    }

    public static String run() throws Exception {
        StringBuilder out = new StringBuilder();
        CharsetDecoder decoder = StandardCharsets.UTF_8.newDecoder();
        out.append(decoder.malformedInputAction()).append(',').append(decoder.unmappableCharacterAction())
            .append(',').append((int) decoder.replacement().charAt(0)).append(',')
            .append(decoder.maxCharsPerByte()).append('|');
        CharsetDecoder same = decoder.onMalformedInput(CodingErrorAction.IGNORE)
            .onUnmappableCharacter(CodingErrorAction.REPLACE);
        out.append(same == decoder).append(',').append(decoder.malformedInputAction()).append(',')
            .append(decoder.unmappableCharacterAction()).append('|');
        out.append(attempt(() -> decoder.onMalformedInput(null))).append(',')
            .append(attempt(() -> decoder.replaceWith(""))).append(',')
            .append(attempt(() -> decoder.replaceWith("??"))).append(',')
            .append(attempt(() -> decoder.replaceWith("?"))).append(',').append(decoder.replacement()).append('|');
        CharsetDecoder strict = StandardCharsets.UTF_8.newDecoder()
            .onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT);
        BufferedReader reader = new BufferedReader(new InputStreamReader(
            Probe.class.getResourceAsStream("/settings.toml"), strict));
        out.append(reader.readLine());
        return out.toString();
    }

}
