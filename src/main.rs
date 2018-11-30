extern crate gtk;
extern crate dirs;
#[macro_use] extern crate serde_derive;
extern crate serde_json;
extern crate rpassword;
extern crate clap;
extern crate gio;

use gtk::prelude::*;
use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use clap::{App, Arg};
use gtk::{Button, ApplicationWindow, Builder, Entry, SettingsExt};
use std::cell::RefCell;
use gio::{ApplicationExt, ApplicationExtManual};

macro_rules! gtk_clone {
    (@param _) => ( _ );
    (@param $x:ident) => ( $x );
    ($($n:ident),+ => move || $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move || $body
        }
    );
    ($($n:ident),+ => move |$($p:tt),+| $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move |$(gtk_clone!(@param $p),)+| $body
        }
    );
}

thread_local!(
    static GLOBAL_STATE: RefCell<State> = RefCell::new(State::default());
);

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    args: Vec<String>,
    default_user: String,
    default_server: String,
}

#[derive(Default)]
struct State {
    user: String,
    server: String,
    password: String,
}

const CONFIG_FILE_NAME: &'static str = "rdp-config.json";
const COMMAND_TO_RUN: &'static str = "xfreerdp";
const VERSION: &'static str = env!("CARGO_PKG_VERSION");

const ARG_MODE: &'static str = "ARG_MODE";
const ARG_SERVER: &'static str = "ARG_SERVER";
const ARG_USER: &'static str = "ARG_USER";

const ARG_MODE_GUI: &'static str = "gui";
const ARG_MODE_CLI: &'static str = "cli";

#[derive(Debug)]
enum Mode {
    Gui,
    Cli
}

fn parse_mode(mode: &str) -> Option<Mode> {
    if mode == ARG_MODE_CLI {
        Some(Mode::Cli)
    } else if mode == ARG_MODE_GUI {
        Some(Mode::Gui)
    } else {
        None
    }
}

fn get_config() -> Config {
    let home_dir = dirs::home_dir().expect("Cannot get home directory location");
    let config_file = PathBuf::from(home_dir).join(CONFIG_FILE_NAME);

    let mut f = File::open(config_file).expect("Config file not found");

    let mut contents = String::new();
    f.read_to_string(&mut contents).expect("Cannot read config file");

    serde_json::from_str(&contents).expect("Cannot deserialize config")
}

fn build_gui(application: &gtk::Application) {
    gtk::Settings::get_default().unwrap().set_property_gtk_application_prefer_dark_theme(true);

    let builder = Builder::new_from_string(include_str!("pwd.glade"));

    let window: ApplicationWindow = builder.get_object("window").expect("Couldn't get window");
    let server: Entry = builder.get_object("inpServer").expect("server control not found");
    let login: Entry = builder.get_object("inpLogin").expect("login control not found");
    let password: Entry = builder.get_object("inpPassword").expect("password control not found");
    let button: Button = builder.get_object("btnOk").expect("OK button control not found");

    window.set_application(application);

    window.connect_delete_event(gtk_clone!(window => move |_, _| {
        window.destroy();
        Inhibit(false)
    }));

    password.grab_focus_without_selecting();

    GLOBAL_STATE.with(|state| {
        login.set_text(&state.borrow_mut().user);
        server.set_text(&state.borrow_mut().server);
        password.set_text(&state.borrow_mut().password);
    });

    let data_providers = (login.clone(), server.clone(), password.clone(), window.clone());

    button.connect_clicked(gtk_clone!(data_providers => move |_| {
        GLOBAL_STATE.with(|state| {
            state.borrow_mut().user = data_providers.0.get_text().unwrap();
            state.borrow_mut().server = data_providers.1.get_text().unwrap();
            state.borrow_mut().password = data_providers.2.get_text().unwrap();
            data_providers.3.destroy();
            Inhibit(false)
        });
    }));

    window.show_all();
}

fn run_gui() {
    let application = gtk::Application::new("com.automatl.rdp", gio::ApplicationFlags::empty())
        .expect("Initialization failed...");

    application.connect_startup(move |app| {
        build_gui(app);
    });

    application.connect_activate(|_| {});
    application.run(&[String::from("")]);
}

fn run_cli() {
    let pass = rpassword::prompt_password_stdout("Password: ").unwrap();
    GLOBAL_STATE.with(|state| {
        state.borrow_mut().password = pass;
    });
}

fn main() {
    let config = get_config();

    let matches = App::new("rdp")
        .version(VERSION)
        .author("Alexander Automatl <ya@tomatl.org>")
        .about("RDP launcher")
        .arg(Arg::with_name(ARG_SERVER)
            .help("Server to connect to")
            .required(true)
            .takes_value(true)
            .default_value(&config.default_server)
            .short("s")
            .long("server"))
        .arg(Arg::with_name(ARG_USER)
            .help("Username to use")
            .required(true)
            .takes_value(true)
            .default_value(&config.default_user)
            .short("u")
            .long("user"))
        .arg(Arg::with_name(ARG_MODE)
            .help("Mode, one of [gui, cli]")
            .required(true)
            .takes_value(true)
            .default_value(ARG_MODE_CLI)
            .short("m")
            .long("mode"))
        .get_matches();

    let mut args = config.args.clone();

    let mode = matches.value_of(ARG_MODE).and_then(|x| parse_mode(x)).expect("Invalid mode parameter");

    GLOBAL_STATE.with(|state| {
        state.borrow_mut().server = matches.value_of(ARG_SERVER).expect("Cannot get server parameter").to_owned();
        state.borrow_mut().user = matches.value_of(ARG_USER).expect("Cannot get user parameter").to_owned();
    });

    let cmd = COMMAND_TO_RUN;

    match mode {
        Mode::Cli => run_cli(),
        Mode::Gui => run_gui(),
    };

    GLOBAL_STATE.with(|state| {
        args.push(String::from("/p:") + &state.borrow().password);
        args.push(String::from("/u:") + &state.borrow().user);
        args.push(String::from("/v:") + &state.borrow().server);
    });

    Command::new(cmd)
        .args(&args)
        .stdout(Stdio::null())
        //.stderr(Stdio::null())
        .spawn()
        .expect("Cannot run RDP process");
}
