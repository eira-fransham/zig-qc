//! subs.qc
//!
//! Copyright (C) 1996-1997  Id Software, Inc.
//!
//! This program is free software; you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the Free
//! Software Foundation; either version 2 of the License, or (at your option)
//! any later version. See gnu.txt.

const q = @import("defs.zig");
const g = @import("game.zig");
const S = q.S;
const Entity = q.Entity;
const Vector = q.Vector;

pub fn SUB_Null() callconv(.c) void {}

pub fn SUB_Remove() callconv(.c) void {
    q.remove(q.me());
}

/// QuakeEd only writes a single float for angles (bad idea), so up and down
/// are just constant angles.
///
/// (Declared `vector()` in the original, but it never returns a value.)
pub fn SetMovedir() callconv(.c) void {
    const self = q.me();
    if (q.veq(self.angles, q.vec(0, -1, 0))) {
        self.movedir = q.vec(0, 0, 1);
    } else if (q.veq(self.angles, q.vec(0, -2, 0))) {
        self.movedir = q.vec(0, 0, -1);
    } else {
        q.makevectors(self.angles);
        self.movedir = q.v_forward.get();
    }

    self.angles = q.vec(0, 0, 0);
}

/// InitTrigger
pub fn InitTrigger() callconv(.c) void {
    const self = q.me();
    // trigger angles are used for one-way touches.  An angle of 0 is assumed
    // to mean no restrictions, so use a yaw of 360 instead.
    if (!q.veq(self.angles, q.vec(0, 0, 0)))
        SetMovedir();
    self.solid = q.SOLID_TRIGGER;
    q.setmodel(self, self.model); // set size and link into world
    self.movetype = q.MOVETYPE_NONE;
    self.modelindex = 0;
    self.model = S("");
}

/// SUB_CalcMove
///
/// calculate self.velocity and self.nextthink to reach dest from
/// self.origin traveling at speed
pub fn SUB_CalcMoveEnt(ent: Entity, tdest: Vector, tspeed: f32, func: q.Function) callconv(.c) void {
    const stemp = q.self.get();
    q.self.set(ent);

    SUB_CalcMove(tdest, tspeed, func);
    q.self.set(stemp);
}

pub fn SUB_CalcMove(tdest: Vector, tspeed: f32, func: q.Function) callconv(.c) void {
    const self = q.me();

    if (tspeed == 0)
        q.objerror(S("No speed is defined!"));

    self.set(g.think1, func);
    self.set(g.finaldest, tdest);
    self.think = q.func(SUB_CalcMoveDone);

    if (q.veq(tdest, self.origin)) {
        self.velocity = q.vec(0, 0, 0);
        self.nextthink = self.ltime + 0.1;
        return;
    }

    // set destdelta to the vector needed to move
    const vdestdelta = tdest - self.origin;

    // calculate length of vector
    const len = q.vlen(vdestdelta);

    // divide by speed to get time to reach dest
    var traveltime = len / tspeed;

    if (traveltime < 0.03)
        traveltime = 0.03;

    // set nextthink to trigger a think when dest is reached
    self.nextthink = self.ltime + traveltime;

    // scale the destdelta vector by the time spent traveling to get velocity
    self.velocity = q.scale(vdestdelta, 1 / traveltime); // qcc won't take vec/float
}

/// After moving, set origin to exact final destination
pub fn SUB_CalcMoveDone() callconv(.c) void {
    const self = q.me();
    q.setorigin(self, self.get(g.finaldest));
    self.velocity = q.vec(0, 0, 0);
    self.nextthink = -1;
    if (!q.fnIsNull(self.get(g.think1)))
        self.get(g.think1).?();
}

/// SUB_CalcAngleMove
///
/// calculate self.avelocity and self.nextthink to reach destangle from
/// self.angles rotating
///
/// The calling function should make sure self.think is valid
pub fn SUB_CalcAngleMoveEnt(ent: Entity, destangle: Vector, tspeed: f32, func: q.Function) callconv(.c) void {
    const stemp = q.self.get();
    q.self.set(ent);
    SUB_CalcAngleMove(destangle, tspeed, func);
    q.self.set(stemp);
}

pub fn SUB_CalcAngleMove(destangle: Vector, tspeed: f32, func: q.Function) callconv(.c) void {
    const self = q.me();

    if (tspeed == 0)
        q.objerror(S("No speed is defined!"));

    // set destdelta to the vector needed to move
    const destdelta = destangle - self.angles;

    // calculate length of vector
    const len = q.vlen(destdelta);

    // divide by speed to get time to reach dest
    const traveltime = len / tspeed;

    // set nextthink to trigger a think when dest is reached
    self.nextthink = self.ltime + traveltime;

    // scale the destdelta vector by the time spent traveling to get velocity
    self.avelocity = q.scale(destdelta, 1 / traveltime);

    self.set(g.think1, func);
    self.set(g.finalangle, destangle);
    self.think = q.func(SUB_CalcAngleMoveDone);
}

/// After rotating, set angle to exact final angle
pub fn SUB_CalcAngleMoveDone() callconv(.c) void {
    const self = q.me();
    self.angles = self.get(g.finalangle);
    self.avelocity = q.vec(0, 0, 0);
    self.nextthink = -1;
    if (!q.fnIsNull(self.get(g.think1)))
        self.get(g.think1).?();
}

//=============================================================================

pub fn DelayThink() callconv(.c) void {
    const self = q.me();
    g.activator.set(self.enemy);
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
pub fn SUB_UseTargets() callconv(.c) void {
    const self = q.me();

    //
    // check for a delay
    //
    if (self.get(g.delay) != 0) {
        // create a temp object to fire at a later time
        const t = q.spawn();
        t.classname = S("DelayedUse");
        t.nextthink = q.time.get() + self.get(g.delay);
        t.think = q.func(DelayThink);
        t.enemy = g.activator.get();
        t.message = self.message;
        t.set(g.killtarget, self.get(g.killtarget));
        t.target = self.target;
        return;
    }

    //
    // print the message
    //
    const activator = g.activator.get();
    if (activator.classname.eql(S("player")) and self.message.neq(S(""))) {
        q.centerprint(activator, self.message);
        if (self.noise.isNull())
            q.sound(activator, q.CHAN_VOICE, S("misc/talk.wav"), 1, q.ATTN_NORM);
    }

    //
    // kill the killtagets
    //
    if (!self.get(g.killtarget).isNull()) {
        var t = q.world.get();
        while (true) {
            t = q.find(t, .targetname, self.get(g.killtarget));
            if (t.isNull())
                return;
            q.remove(t);
        }
    }

    //
    // fire targets
    //
    if (!self.target.isNull()) {
        const act = g.activator.get();
        var t = q.world.get();
        while (true) {
            t = q.find(t, .targetname, self.target);
            if (t.isNull()) {
                return;
            }
            const stemp = q.self.get();
            const otemp = q.other.get();
            q.self.set(t);
            q.other.set(stemp);
            if (q.fnNeq(t.use, q.func(SUB_Null))) {
                if (!q.fnIsNull(t.use))
                    t.use.?();
            }
            q.self.set(stemp);
            q.other.set(otemp);
            g.activator.set(act);
        }
    }
}
