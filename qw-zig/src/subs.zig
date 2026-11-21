//! subs.qc
//!
//! Copyright (C) 1996-1997  Id Software, Inc.
//!
//! This program is free software; you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the Free
//! Software Foundation; either version 2 of the License, or (at your option)
//! any later version. See gnu.txt.

const q = @import("defs.zig");
const S = q.S;
const Entity = q.Entity;
const Vector = q.Vector;

pub export fn SUB_Null() void {}

pub export fn SUB_Remove() void {
    q.remove(q.me());
}

/// QuakeEd only writes a single float for angles (bad idea), so up and down
/// are just constant angles.
///
/// (Declared `vector()` in the original, but it never returns a value.)
pub export fn SetMovedir() void {
    const self = q.me();
    if (q.veq(self.get(.angles), q.vec(0, -1, 0))) {
        self.set(.movedir, q.vec(0, 0, 1));
    } else if (q.veq(self.get(.angles), q.vec(0, -2, 0))) {
        self.set(.movedir, q.vec(0, 0, -1));
    } else {
        q.makevectors(self.get(.angles));
        self.set(.movedir, q.v_forward.get());
    }

    self.set(.angles, q.vec(0, 0, 0));
}

/// InitTrigger
pub export fn InitTrigger() void {
    const self = q.me();
    // trigger angles are used for one-way touches.  An angle of 0 is assumed
    // to mean no restrictions, so use a yaw of 360 instead.
    if (!q.veq(self.get(.angles), q.vec(0, 0, 0)))
        SetMovedir();
    self.set(.solid, q.SOLID_TRIGGER);
    q.setmodel(self, self.get(.model)); // set size and link into world
    self.set(.movetype, q.MOVETYPE_NONE);
    self.set(.modelindex, 0);
    self.set(.model, S(""));
}

/// SUB_CalcMove
///
/// calculate self.velocity and self.nextthink to reach dest from
/// self.origin traveling at speed
pub export fn SUB_CalcMoveEnt(ent: Entity, tdest: Vector, tspeed: f32, func: q.Function) void {
    const stemp = q.self.get();
    q.self.set(ent);

    SUB_CalcMove(tdest, tspeed, func);
    q.self.set(stemp);
}

pub export fn SUB_CalcMove(tdest: Vector, tspeed: f32, func: q.Function) void {
    const self = q.me();

    if (tspeed == 0)
        q.objerror(S("No speed is defined!"));

    self.set(q.think1, func);
    self.set(q.finaldest, tdest);
    self.set(.think, q.func(SUB_CalcMoveDone));

    if (q.veq(tdest, self.get(.origin))) {
        self.set(.velocity, q.vec(0, 0, 0));
        self.set(.nextthink, self.get(.ltime) + 0.1);
        return;
    }

    // set destdelta to the vector needed to move
    const vdestdelta = tdest - self.get(.origin);

    // calculate length of vector
    const len = q.vlen(vdestdelta);

    // divide by speed to get time to reach dest
    var traveltime = len / tspeed;

    if (traveltime < 0.03)
        traveltime = 0.03;

    // set nextthink to trigger a think when dest is reached
    self.set(.nextthink, self.get(.ltime) + traveltime);

    // scale the destdelta vector by the time spent traveling to get velocity
    self.set(.velocity, q.scale(vdestdelta, 1 / traveltime)); // qcc won't take vec/float
}

/// After moving, set origin to exact final destination
pub export fn SUB_CalcMoveDone() void {
    const self = q.me();
    q.setorigin(self, self.get(q.finaldest));
    self.set(.velocity, q.vec(0, 0, 0));
    self.set(.nextthink, -1);
    if (!q.fnIsNull(self.get(q.think1)))
        self.get(q.think1)();
}

/// SUB_CalcAngleMove
///
/// calculate self.avelocity and self.nextthink to reach destangle from
/// self.angles rotating
///
/// The calling function should make sure self.think is valid
pub export fn SUB_CalcAngleMoveEnt(ent: Entity, destangle: Vector, tspeed: f32, func: q.Function) void {
    const stemp = q.self.get();
    q.self.set(ent);
    SUB_CalcAngleMove(destangle, tspeed, func);
    q.self.set(stemp);
}

pub export fn SUB_CalcAngleMove(destangle: Vector, tspeed: f32, func: q.Function) void {
    const self = q.me();

    if (tspeed == 0)
        q.objerror(S("No speed is defined!"));

    // set destdelta to the vector needed to move
    const destdelta = destangle - self.get(.angles);

    // calculate length of vector
    const len = q.vlen(destdelta);

    // divide by speed to get time to reach dest
    const traveltime = len / tspeed;

    // set nextthink to trigger a think when dest is reached
    self.set(.nextthink, self.get(.ltime) + traveltime);

    // scale the destdelta vector by the time spent traveling to get velocity
    self.set(.avelocity, q.scale(destdelta, 1 / traveltime));

    self.set(q.think1, func);
    self.set(q.finalangle, destangle);
    self.set(.think, q.func(SUB_CalcAngleMoveDone));
}

/// After rotating, set angle to exact final angle
pub export fn SUB_CalcAngleMoveDone() void {
    const self = q.me();
    self.set(.angles, self.get(q.finalangle));
    self.set(.avelocity, q.vec(0, 0, 0));
    self.set(.nextthink, -1);
    if (!q.fnIsNull(self.get(q.think1)))
        self.get(q.think1)();
}

//=============================================================================

pub export fn DelayThink() void {
    const self = q.me();
    q.activator.set(self.get(.enemy));
    SUB_UseTargets();
    q.remove(self);
}

/// SUB_UseTargets
///
/// the global "activator" should be set to the entity that initiated the
/// firing.
///
/// If self.delay is set, a DelayedUse entity will be created that will
/// actually do the SUB_UseTargets after that many seconds have passed.
///
/// Centerprints any self.message to the activator.
///
/// Removes all entities with a targetname that match self.killtarget, and
/// removes them, so some events can remove other triggers.
///
/// Search for (string)targetname in all entities that match
/// (string)self.target and call their .use function
pub export fn SUB_UseTargets() void {
    const self = q.me();

    //
    // check for a delay
    //
    if (self.get(q.delay) != 0) {
        // create a temp object to fire at a later time
        const t = q.spawn();
        t.set(.classname, S("DelayedUse"));
        t.set(.nextthink, q.time.get() + self.get(q.delay));
        t.set(.think, q.func(DelayThink));
        t.set(.enemy, q.activator.get());
        t.set(.message, self.get(.message));
        t.set(q.killtarget, self.get(q.killtarget));
        t.set(.target, self.get(.target));
        return;
    }

    //
    // print the message
    //
    const activator = q.activator.get();
    if (activator.get(.classname).eql(S("player")) and self.get(.message).neq(S(""))) {
        q.centerprint(activator, self.get(.message));
        if (self.get(.noise).isNull())
            q.sound(activator, q.CHAN_VOICE, S("misc/talk.wav"), 1, q.ATTN_NORM);
    }

    //
    // kill the killtagets
    //
    if (!self.get(q.killtarget).isNull()) {
        var t = q.world.get();
        while (true) {
            t = q.find(t, q.fieldRef(.targetname), self.get(q.killtarget));
            if (t.isNull())
                return;
            q.remove(t);
        }
    }

    //
    // fire targets
    //
    if (!self.get(.target).isNull()) {
        const act = q.activator.get();
        var t = q.world.get();
        while (true) {
            t = q.find(t, q.fieldRef(.targetname), self.get(.target));
            if (t.isNull()) {
                return;
            }
            const stemp = q.self.get();
            const otemp = q.other.get();
            q.self.set(t);
            q.other.set(stemp);
            if (q.fnNeq(t.get(.use), q.func(SUB_Null))) {
                if (!q.fnIsNull(t.get(.use)))
                    t.get(.use)();
            }
            q.self.set(stemp);
            q.other.set(otemp);
            q.activator.set(act);
        }
    }
}
