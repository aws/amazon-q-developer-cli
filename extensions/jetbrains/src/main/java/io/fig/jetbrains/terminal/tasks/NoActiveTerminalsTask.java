package io.fig.jetbrains.terminal.tasks;

import io.fig.jetbrains.FigService;

import java.util.TimerTask;

public class NoActiveTerminalsTask extends TimerTask {

    private final FigService service;

    public NoActiveTerminalsTask(FigService service) {
        this.service = service;
    }

    @Override
    public void run() {
        boolean anyTerminalFocused = this.service.isAnyTerminalFocused();

        if (!anyTerminalFocused) {
            System.out.println("no active terminals");
            // this.service.runCommand("fig bg:jetbrains-no-active-terminals");
        }
    }
}
