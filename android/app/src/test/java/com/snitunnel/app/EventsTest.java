package com.snitunnel.app;
import org.junit.Test;
import static org.junit.Assert.*;
public class EventsTest {
    @Test public void credentialsAreRedacted(){String value=Events.clean("id 00000000-0000-4000-8000-000000000001 vmess://encoded-credentials failed");assertFalse(value.contains("00000000"));assertFalse(value.contains("encoded-credentials"));}
    @Test public void historyIsBounded(){Events.clear();for(int n=0;n<150;n++)Events.add("event "+n);assertFalse(Events.text().contains("  event 0\n"));assertTrue(Events.text().contains("event 149"));}
}
