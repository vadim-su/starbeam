use bevy::prelude::*;
use statig::prelude::*;

use crate::enemy::components::*;
use crate::physics::Velocity;
use crate::player::Player;

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Events that drive the AI state machine.
pub enum AiEvent {
    /// A player is within detection range.
    PlayerInRange(Vec2),
    /// The player left detection range.
    PlayerOutOfRange,
    /// The player is within attack range.
    InAttackRange(Vec2),
    /// A simulation tick: (dt, current_position).
    Tick(f32, Vec2),
    /// The enemy has reached its patrol anchor.
    ReachedAnchor,
}

// ---------------------------------------------------------------------------
// Shared storage (the statig "self" struct)
// ---------------------------------------------------------------------------

/// Shared data available to every state handler.
pub struct EnemyAi {
    /// Where the enemy should return to / patrol around.
    pub anchor: Vec2,
    /// Movement speed in pixels per second.
    pub speed: f32,
    /// Last known player position (set when chasing / attacking).
    pub target: Option<Vec2>,
    /// Desired velocity output — read by the Bevy system after handling.
    pub desired_velocity: Vec2,
}

// ---------------------------------------------------------------------------
// State machine definition (statig 0.3)
// ---------------------------------------------------------------------------

#[state_machine(initial = "State::idle()")]
impl EnemyAi {
    // -- Idle ---------------------------------------------------------------

    #[state]
    fn idle(&mut self, event: &AiEvent) -> Response<State> {
        match event {
            AiEvent::PlayerInRange(pos) => {
                self.target = Some(*pos);
                Transition(State::chase())
            }
            AiEvent::Tick(..) => {
                self.desired_velocity = Vec2::ZERO;
                Handled
            }
            _ => Super,
        }
    }

    // -- Patrol -------------------------------------------------------------

    #[state]
    fn patrol(&mut self, event: &AiEvent) -> Response<State> {
        match event {
            AiEvent::PlayerInRange(pos) => {
                self.target = Some(*pos);
                Transition(State::chase())
            }
            AiEvent::ReachedAnchor => Transition(State::idle()),
            AiEvent::Tick(_dt, pos) => {
                let dir = (self.anchor - *pos).normalize_or_zero();
                self.desired_velocity = dir * self.speed * 0.5;
                Handled
            }
            _ => Super,
        }
    }

    // -- Chase --------------------------------------------------------------

    #[state]
    fn chase(&mut self, event: &AiEvent) -> Response<State> {
        match event {
            AiEvent::PlayerOutOfRange => Transition(State::return_to_anchor()),
            AiEvent::InAttackRange(pos) => {
                self.target = Some(*pos);
                Transition(State::attack())
            }
            AiEvent::PlayerInRange(pos) => {
                self.target = Some(*pos);
                Handled
            }
            AiEvent::Tick(_dt, pos) => {
                if let Some(target) = self.target {
                    let dir = (target - *pos).normalize_or_zero();
                    self.desired_velocity = dir * self.speed;
                }
                Handled
            }
            _ => Super,
        }
    }

    // -- Attack -------------------------------------------------------------

    #[state]
    fn attack(&mut self, event: &AiEvent) -> Response<State> {
        match event {
            AiEvent::PlayerOutOfRange => Transition(State::chase()),
            AiEvent::InAttackRange(pos) => {
                self.target = Some(*pos);
                Handled
            }
            AiEvent::Tick(..) => {
                // Stay roughly in place while attacking
                self.desired_velocity = Vec2::ZERO;
                Handled
            }
            _ => Super,
        }
    }

    // -- Return to anchor ---------------------------------------------------

    #[state]
    fn return_to_anchor(&mut self, event: &AiEvent) -> Response<State> {
        match event {
            AiEvent::PlayerInRange(pos) => {
                self.target = Some(*pos);
                Transition(State::chase())
            }
            AiEvent::ReachedAnchor => Transition(State::idle()),
            AiEvent::Tick(_dt, pos) => {
                let dir = (self.anchor - *pos).normalize_or_zero();
                self.desired_velocity = dir * self.speed * 0.75;
                Handled
            }
            _ => Super,
        }
    }
}

// ---------------------------------------------------------------------------
// Bevy Component wrapper
// ---------------------------------------------------------------------------

/// Bevy component that wraps the statig state machine.
#[derive(Component)]
pub struct AiStateMachine {
    pub machine: StateMachine<EnemyAi>,
}

impl AiStateMachine {
    pub fn new(anchor: Vec2, speed: f32) -> Self {
        let ai = EnemyAi {
            anchor,
            speed,
            target: None,
            desired_velocity: Vec2::ZERO,
        };
        let mut machine = ai.state_machine();
        machine.init();
        Self { machine }
    }
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

/// Tick every enemy AI state machine each frame.
pub fn enemy_ai_tick(
    time: Res<Time>,
    player_query: Query<&Transform, With<Player>>,
    mut enemy_query: Query<
        (
            &Transform,
            &mut Velocity,
            &mut AiStateMachine,
            &DetectionRange,
            &AttackRange,
            &PatrolAnchor,
            &MoveSpeed,
        ),
        With<Enemy>,
    >,
) {
    let dt = time.delta_secs();
    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    for (tf, mut vel, mut ai, detection, attack, _patrol, _speed) in &mut enemy_query {
        let enemy_pos = tf.translation.truncate();
        let dist = enemy_pos.distance(player_pos);

        // Send the most specific event first
        if dist <= attack.0 {
            ai.machine.handle(&AiEvent::InAttackRange(player_pos));
        } else if dist <= detection.0 {
            ai.machine.handle(&AiEvent::PlayerInRange(player_pos));
        } else {
            ai.machine.handle(&AiEvent::PlayerOutOfRange);
        }

        // Check if close enough to anchor for ReachedAnchor
        let anchor = ai.machine.anchor;
        if enemy_pos.distance(anchor) < 8.0 {
            ai.machine.handle(&AiEvent::ReachedAnchor);
        }

        // Tick for velocity computation (passes current position)
        ai.machine.handle(&AiEvent::Tick(dt, enemy_pos));

        // Apply desired velocity
        let desired = ai.machine.desired_velocity;
        vel.x = desired.x;
        vel.y = desired.y;
    }
}
