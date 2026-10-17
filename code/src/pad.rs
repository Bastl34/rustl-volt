// the game buttons of one frame: the keyboard and every connected gamepad - the driving itself goes through the bindings of the vehicle controller
// pad: D-pad / left stick = arrows, A = Enter, B = Backspace, Start = Esc (pause), X / RB = fire, Back = reset onto the track, LB = watch the bots
// split screen: the buttons of each device on their own (Device), two players can share the keyboard (WASD / arrows)

use std::collections::BTreeMap;

use rustl::input::gamepad::{GamepadAxis, GamepadButton};
use rustl::input::input_binding::{GamepadSelect, InputAction, InputSource};
use rustl::input::input_manager::InputManager;
use rustl::input::keyboard::{Key, Modifier};
use rustl::state::scene::scene_controller::vehicle_controller::VehicleControls;

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

impl Buttons
{
    fn or(self, other: Buttons) -> Buttons
    {
        Buttons
        {
            up: self.up | other.up, down: self.down | other.down, left: self.left | other.left, right: self.right | other.right,
            enter: self.enter | other.enter, back: self.back | other.back, pause: self.pause | other.pause, fire: self.fire | other.fire,
            reset: self.reset | other.reset, watch: self.watch | other.watch, split: self.split | other.split, start: self.start | other.start,
            key_enter: self.key_enter | other.key_enter, key_back: self.key_back | other.key_back,
        }
    }
}

// the input device of a local player: the whole keyboard, its left half (WASD), its right half (arrows) or a gamepad by its player id
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Device
{
    Keyboard,
    KeyboardLeft,
    KeyboardRight,
    Gamepad(usize),
}

impl Device
{
    pub fn label(self) -> String
    {
        match self
        {
            Device::Keyboard => "Keyboard".to_string(),
            Device::KeyboardLeft => "Keyboard WASD".to_string(),
            Device::KeyboardRight => "Keyboard Arrows".to_string(),
            Device::Gamepad(id) => format!("Gamepad {id}"),
        }
    }

    pub fn is_keyboard(self) -> bool
    {
        !matches!(self, Device::Gamepad(_))
    }

    // two players can not have these together: the same device, or the whole keyboard and one of its halves
    pub fn conflicts(self, other: Device) -> bool
    {
        self == other || (self.is_keyboard() && other.is_keyboard() && (self == Device::Keyboard || other == Device::Keyboard))
    }

    // the name in the save
    pub fn key(self) -> String
    {
        match self
        {
            Device::Keyboard => "keyboard".to_string(),
            Device::KeyboardLeft => "wasd".to_string(),
            Device::KeyboardRight => "arrows".to_string(),
            Device::Gamepad(id) => format!("pad{id}"),
        }
    }

    pub fn from_key(key: &str) -> Option<Device>
    {
        match key
        {
            "keyboard" => Some(Device::Keyboard),
            "wasd" => Some(Device::KeyboardLeft),
            "arrows" => Some(Device::KeyboardRight),
            _ => key.strip_prefix("pad").and_then(|id| id.parse().ok()).map(Device::Gamepad),
        }
    }

    // the driving bindings of this device alone - the whole keyboard drives with both halves
    pub fn controls(self) -> VehicleControls
    {
        use InputSource::{Key as K, Modifier as M};
        let action = |sources: &[InputSource]| InputAction::new(sources);
        let mut controls = VehicleControls::default();
        match self
        {
            Device::Keyboard => controls.gamepad = GamepadSelect::None,
            Device::KeyboardLeft =>
            {
                controls = without_gamepad(controls);
                controls.throttle = action(&[K(Key::W)]);
                controls.brake = action(&[K(Key::S)]);
                controls.steer_left = action(&[K(Key::A)]);
                controls.steer_right = action(&[K(Key::D)]);
                controls.handbrake = action(&[K(Key::Space)]);
                controls.recover = action(&[K(Key::R)]);
                controls.camera = action(&[K(Key::C)]);
            },
            Device::KeyboardRight =>
            {
                controls = without_gamepad(controls);
                controls.throttle = action(&[K(Key::ArrowUp)]);
                controls.brake = action(&[K(Key::ArrowDown)]);
                controls.steer_left = action(&[K(Key::ArrowLeft)]);
                controls.steer_right = action(&[K(Key::ArrowRight)]);
                controls.handbrake = action(&[M(Modifier::RightShift)]);
                // not Enter: the menu buttons read it once per frame and would take the press away from the binding
                controls.recover = action(&[K(Key::Delete)]);
                // End above the arrows - AltGr comes as a key or as the right Alt depending on the layout
                controls.camera = action(&[K(Key::End), K(Key::AltGraph), M(Modifier::RightAlt)]);
            },
            Device::Gamepad(id) =>
            {
                let pad_only = |action: &mut InputAction| action.sources.retain(|source| matches!(source, InputSource::GamepadButton(_) | InputSource::GamepadAxis(..)));
                for action in [&mut controls.throttle, &mut controls.brake, &mut controls.steer_left, &mut controls.steer_right, &mut controls.handbrake, &mut controls.recover, &mut controls.camera]
                {
                    pad_only(action);
                }
                controls.gamepad = GamepadSelect::Player(id);
            },
        }
        controls
    }
}

fn without_gamepad(mut controls: VehicleControls) -> VehicleControls
{
    controls.gamepad = GamepadSelect::None;
    controls
}

#[derive(Default)]
pub struct Pad
{
    // stick directions held last frame per gamepad id: up, down, left, right
    stick: BTreeMap<usize, [bool; 4]>,
    pub now: Buttons,
    // the keyboard alone: whole, left half, right half - the halves share the menu keys
    keyboard: Buttons,
    keyboard_left: Buttons,
    keyboard_right: Buttons,
    // per connected gamepad id
    pads: BTreeMap<usize, Buttons>,
}

impl Pad
{
    // once per frame, before anything reads the buttons - a press is used up when it is read
    // race_keys: B, V, P, Q, E are game keys (watch, split view, pause, fire and reset of the left half) - in the menu they are letters of the name
    pub fn update(&mut self, input: &mut InputManager, race_keys: bool)
    {
        let keyboard = &mut input.keyboard;
        let key_enter = keyboard.is_pressed(Key::Enter);
        let key_back = keyboard.is_pressed(Key::Backspace);
        let left_ctrl = keyboard.is_pressed_modifier(Modifier::LeftCtrl);
        let right_ctrl = keyboard.is_pressed_modifier(Modifier::RightCtrl);
        let ctrl = keyboard.is_pressed(Key::Control);
        let keys = Buttons
        {
            up: keyboard.is_pressed(Key::ArrowUp),
            down: keyboard.is_pressed(Key::ArrowDown),
            left: keyboard.is_pressed(Key::ArrowLeft),
            right: keyboard.is_pressed(Key::ArrowRight),
            enter: key_enter,
            back: key_back,
            pause: keyboard.is_pressed(Key::Escape) | (race_keys && keyboard.is_pressed(Key::P)),
            watch: race_keys && keyboard.is_pressed(Key::B),
            split: race_keys && keyboard.is_pressed(Key::V),
            key_enter,
            key_back,
            ..Buttons::default()
        };
        let (left_fire, left_reset) = if race_keys { (keyboard.is_pressed(Key::Q), keyboard.is_pressed(Key::E)) } else { (false, false) };
        self.keyboard = Buttons { fire: left_ctrl | right_ctrl | ctrl, reset: key_back, ..keys };
        self.keyboard_left = Buttons { fire: left_fire, reset: left_reset, ..keys };
        self.keyboard_right = Buttons { fire: right_ctrl, reset: key_back, ..keys };

        self.pads.clear();
        for gamepad in input.gamepads.values_mut().filter(|gamepad| gamepad.connected)
        {
            let start = gamepad.is_pressed(GamepadButton::Start);
            let mut buttons = Buttons
            {
                up: gamepad.is_pressed(GamepadButton::DPadUp),
                down: gamepad.is_pressed(GamepadButton::DPadDown),
                left: gamepad.is_pressed(GamepadButton::DPadLeft),
                right: gamepad.is_pressed(GamepadButton::DPadRight),
                enter: gamepad.is_pressed(GamepadButton::South),
                back: gamepad.is_pressed(GamepadButton::East),
                pause: start,
                start,
                fire: gamepad.is_pressed(GamepadButton::West) | gamepad.is_pressed(GamepadButton::RightBumper),
                reset: gamepad.is_pressed(GamepadButton::Select),
                watch: gamepad.is_pressed(GamepadButton::LeftBumper),
                ..Buttons::default()
            };

            // the stick: a press when it crosses the threshold, held until it is back near the middle
            let (x, y) = (gamepad.axes[GamepadAxis::LeftStickX as usize], gamepad.axes[GamepadAxis::LeftStickY as usize]);
            let held = self.stick.entry(gamepad.id).or_default();
            let pushed = [y, -y, -x, x];
            let mut fresh = [false; 4];
            for (i, value) in pushed.iter().enumerate()
            {
                if *value > STICK_PRESS && !held[i] { fresh[i] = true; held[i] = true; }
                if *value < STICK_PRESS * 0.5 { held[i] = false; }
            }
            buttons.up |= fresh[0];
            buttons.down |= fresh[1];
            buttons.left |= fresh[2];
            buttons.right |= fresh[3];
            self.pads.insert(gamepad.id, buttons);
        }

        self.now = self.pads.values().fold(self.keyboard.or(self.keyboard_left).or(self.keyboard_right), |all, pad| all.or(*pad));
    }

    // the buttons of one device this frame - nothing for a gamepad that is not connected
    pub fn device(&self, device: Device) -> Buttons
    {
        match device
        {
            Device::Keyboard => self.keyboard,
            Device::KeyboardLeft => self.keyboard_left,
            Device::KeyboardRight => self.keyboard_right,
            Device::Gamepad(id) => self.pads.get(&id).copied().unwrap_or_default(),
        }
    }
}
