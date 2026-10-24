package com.snitunnel.app;

/** Native forwarder counters reset on recovery; display cumulative bytes and rates. */
final class TrafficMeter {
    private long baseUp,baseDown,lastUp,lastDown,lastTime;
    synchronized void reset(long now){baseUp=baseDown=lastUp=lastDown=0;lastTime=now;}
    synchronized void newEpoch(long now){baseUp+=lastUp;baseDown+=lastDown;lastUp=lastDown=0;lastTime=now;}
    synchronized long[] update(long up,long down,long now){
        up=Math.max(0,up);down=Math.max(0,down);
        if(up<lastUp||down<lastDown)newEpoch(now);
        long elapsed=Math.max(0,now-lastTime),upRate=0,downRate=0;
        if(elapsed>0){upRate=(up-lastUp)*1000/elapsed;downRate=(down-lastDown)*1000/elapsed;}
        lastUp=up;lastDown=down;lastTime=now;
        return new long[]{baseUp+up,baseDown+down,upRate,downRate};
    }
}
