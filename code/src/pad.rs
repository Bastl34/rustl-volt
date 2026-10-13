// the game buttons of one frame: the keyboard and every connected gamepad - the driving itself goes through the bindings of the vehicle controller
// pad: D-pad / left stick = arrows, A = Enter, B = Backspace, Start = Esc (pause), X / RB = fire, Back = reset onto the track, LB = watch the bots

use rustl::input::gamepad::{GamepadAxis, GamepadButton};
use rustl::input::input_manager::InputManager;
use rustl::input::keyboard::{Key, Modifier};

// the stick counts as a press once it is pushed this far, and again after it came back below the half of it
const STICK_PRESS: f32 = 0.6;

#[derive(Default, Clone, Copy)]
pub struct Buttons
{
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    // Enter, pad A
    pub enter: bool,
    // Backspace, pad B
    pub back: bool,
    // Esc, pad Start
    pub pause: bool,
    // Ctrl, pad X / RB
    pub fire: bool,
    // Backspace, pad Back
    pub reset: bool,
    // B, pad LB
    pub watch: bool,
    // V
    pub split: bool,
    // pad Start alone - the menus take it as Enter
    pub start: bool,
    // keyboard only: the stunt arena opens its menu with Enter / Esc / Backspace (Esc stops Play in the editor)
    pub key_enter: bool,
    pub key_back: bool,
}

#[derive(Default)]
pub struct Pad
{
    // stick directions held last frame: up, down, left, right
    stick: [bool; 4],
    pub now: Buttons,
}

impl Pad
{
    // once per frame, before anything reads the buttons - a press is used up when it is read
    // race_keys: B and V are game keys (watch, split view) - in the menu they are letters of the name
    pub fn update(&mut self, input: &mut InputManager, race_keys: bool)
    {
        let keyboard = &mut input.keyboard;
        let key_enter = keyboard.is_pressed(Key::Enter);
        let key_back = keyboard.is_pressed(Key::Backspace);
        let mut now = Buttons
        {
            up: keyboard.is_pressed(Key::ArrowUp),
            down: keyboard.is_pressed(Key::ArrowDown),
            left: keyboard.is_pressed(Key::ArrowLeft),
            right: keyboard.is_pressed(Key::ArrowRight),
            enter: key_enter,
            back: key_back,
            pause: keyboard.is_pressed(Key::Escape),
            fire: keyboard.is_pressed_modifier(Modifier::LeftCtrl) | keyboard.is_pressed_modifier(Modifier::RightCtrl) | keyboard.is_pressed(Key::Control),
            reset: key_back,
            watch: race_keys && keyboard.is_pressed(Key::B),
            split: race_keys && keyboard.is_pressed(Key::V),
            start: false,
            key_enter,
            key_back,
        };

        let mut stick = [0.0f32; 2];
        for gamepad in input.gamepads.values_mut().filter(|gamepad| gamepad.connected)
        {
            now.up |= gamepad.is_pressed(GamepadButton::DPadUp);
            now.down |= gamepad.is_pressed(GamepadButton::DPadDown);
            now.left |= gamepad.is_pressed(GamepadButton::DPadLeft);
            now.right |= gamepad.is_pressed(GamepadButton::DPadRight);
            now.enter |= gamepad.is_pressed(GamepadButton::South);
            now.back |= gamepad.is_pressed(GamepadButton::East);
            let start = gamepad.is_pressed(GamepadButton::Start);
            now.pause |= start;
            now.start |= start;
            now.fire |= gamepad.is_pressed(GamepadButton::West) | gamepad.is_pressed(GamepadButton::RightBumper);
            now.reset |= gamepad.is_pressed(GamepadButton::Select);
            now.watch |= gamepad.is_pressed(GamepadButton::LeftBumper);

            let (x, y) = (gamepad.axes[GamepadAxis::LeftStickX as usize], gamepad.axes[GamepadAxis::LeftStickY as usize]);
            if x.abs() > stick[0].abs() { stick[0] = x; }
            if y.abs() > stick[1].abs() { stick[1] = y; }
        }

        // the stick: a press when it crosses the threshold, held until it is back near the middle
        let pushed = [stick[1], -stick[1], -stick[0], stick[0]];
        let mut fresh = [false; 4];
        for (i, value) in pushed.iter().enumerate()
        {
            if *value > STICK_PRESS && !self.stick[i] { fresh[i] = true; self.stick[i] = true; }
            if *value < STICK_PRESS * 0.5 { self.stick[i] = false; }
        }
        now.up |= fresh[0];
        now.down |= fresh[1];
        now.left |= fresh[2];
        now.right |= fresh[3];

        self.now = now;
    }
}
