namespace Fig {
    public static delegate int CursorCallback (int x, int y, int w, int h);
    public static delegate int LogCallback (uint8 level, char *message);

    static CursorCallback cursor_callback;
    static LogCallback log_callback;
    static bool started_by_ibus;

    class FigEngine: IBus.Engine {
        construct {
            log_callback (2, "Engine constructioned");
            this.set_cursor_location.connect ((x, y, w, h) => {
                cursor_callback (x, y, w, h);
            });
        }

        public FigEngine() {
            Object(
                engine_name: "Fig"
            );
        }
    }

    [CCode(cname="fig_engine_main")]
    public void main(bool started_by_ibus_, CursorCallback cursor_callback_, LogCallback log_callback_) {
        started_by_ibus = started_by_ibus_;
        cursor_callback = cursor_callback_;
        log_callback = log_callback_;

        IBus.init ();

        var bus = new IBus.Bus ();
        if (!bus.is_connected ()) {
            log_callback (4, "Could not connect to IBus daemon");
            return;
        }

        bus.disconnected.connect (() => { IBus.quit (); });

        var factory = new IBus.Factory (bus.get_connection ());
        factory.add_engine ("fig", typeof(FigEngine));
        if (started_by_ibus) {
            log_callback (2, "Managed by IBus");
            bus.request_name ("org.freedesktop.IBus.Fig", 0);
        } else {
            log_callback (2, "Not managed by IBus");
            var component = new IBus.Component (
                "org.freedesktop.IBus.Fig", // name
                "Fig IBus integration component", // description
                "0.1.0", // version
                "MIT", // license
                "Fig", // author
                "https://fig.io", // homepage
                "", // command_line
                "" // textdomain
            );
            var desc = new IBus.EngineDesc (
                "fig", // name
                "Fig IBus Engine", // longname 
                "Fig IBus integration engine", // description 
                "", // language
                "MIT", // license 
                "Fig", // author
                "", // icon
                "" // layout
            );
            component.add_engine (desc);
            bus.register_component (component);
        }

        IBus.main ();
    }
}
