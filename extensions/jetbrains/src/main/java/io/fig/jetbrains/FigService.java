package io.fig.jetbrains;

import com.intellij.ide.GeneralSettings;
import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.application.ex.ApplicationEx;
import com.intellij.openapi.wm.ToolWindow;
import com.intellij.openapi.wm.ToolWindowManager;
import com.intellij.terminal.JBTerminalWidget;
import com.intellij.ui.content.Content;
import com.intellij.ui.content.ContentManager;
import io.fig.jetbrains.entries.ITerminalEntry;
import io.fig.jetbrains.entries.impl.ListenTerminalsEntry;
import io.fig.jetbrains.instruments.ITerminalInstrumentation;
import io.fig.jetbrains.instruments.TerminalInstrumentationType;
import io.fig.jetbrains.entries.impl.ExistingTerminalsEntry;
import io.fig.jetbrains.terminal.TerminalStatus;
import io.fig.jetbrains.terminal.listeners.TerminalFocusListener;
import org.jetbrains.plugins.terminal.TerminalView;

import java.io.IOException;
import java.util.HashMap;
import java.util.Map;

public class FigService implements ITerminalInstrumentation {

    private final ITerminalEntry[] terminalEntries = new ITerminalEntry[] {
            new ExistingTerminalsEntry(this),
            new ListenTerminalsEntry(this),
    };
    private final Map<Integer, TerminalStatus> terminals;

    public FigService() {
        this.terminals = new HashMap<>();
        var settings = GeneralSettings.getInstance();
        if (!settings.isSupportScreenReaders()) {
            settings.setSupportScreenReaders(true);
            ((ApplicationEx) ApplicationManager.getApplication()).restart(true);
        }
    }

    public void initContentManager(ToolWindowManager toolWindowManager) {
        ToolWindow terminalWindow = toolWindowManager.getToolWindow("Terminal");
        ContentManager contentManager = null;
        if (terminalWindow != null) {
            contentManager = terminalWindow.getContentManager();
            for (ITerminalEntry terminalEntry : this.terminalEntries)
                terminalEntry.register(contentManager);
        }
    }

    @Override
    public void instrumentTerminalContent(Content content, int id, TerminalInstrumentationType instrumentation) {
        System.out.println("------");

        switch (instrumentation) {
            case ADDED:
                System.out.println("add " + id);
                this.runCommand("q", "hook", "keyboard-focus-changed", "jedi " + id);
                this.terminals.put(id, new TerminalStatus(content, true, id));

                JBTerminalWidget widget = TerminalView.getWidgetByContent(content);

                if (widget == null)
                    return;

                widget.getTerminalPanel().addFocusListener(new TerminalFocusListener(this, content, id));
                break;

            case REMOVED:
                System.out.println("remove " + id);
                this.terminals.remove(id);
                // Should a command be run?
                break;

            case FOCUSED:
                System.out.println("focus " + id);
                this.runCommand("q", "hook", "keyboard-focus-changed", "jedi " + id);
                this.setTerminalFocused(id, true);
                break;

            case UNFOCUSED:
                System.out.println("unfocus " + id);
                this.setTerminalFocused(id, false);

                this.runCommand("q", "hook", "keyboard-focus-changed", "jedi " + id);
                break;

            default:
                break;
        }
    }

    private void setTerminalFocused(int id, boolean focused) {
        TerminalStatus terminal = this.getTerminalFromId(id);

        if (terminal == null)
            return;

        terminal.setFocused(focused);
    }

    private TerminalStatus getTerminalFromId(int id) {
        return this.terminals.get(id);
    }

    public boolean isAnyTerminalFocused() {
        return this.terminals.values().stream().anyMatch(TerminalStatus::isFocused);
    }

    public void runCommand(String... command) {
        try {
            new ProcessBuilder().command(command).start();
        } catch (IOException e) {
            e.printStackTrace();
        }
    }
}
