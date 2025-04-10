package io.fig.jetbrains.terminal.listeners;

import com.intellij.ui.content.Content;
import io.fig.jetbrains.FigService;
import io.fig.jetbrains.instruments.TerminalInstrumentationType;

import java.awt.event.FocusEvent;
import java.awt.event.FocusListener;

public class TerminalFocusListener implements FocusListener {

    private final FigService service;
    private final Content content;
    private final int id;

    public TerminalFocusListener(FigService service, Content content, int id) {
        this.service = service;
        this.content = content;
        this.id = id;
    }

    @Override
    public void focusGained(FocusEvent event) {
        this.runInstrumentation(TerminalInstrumentationType.FOCUSED);
    }

    @Override
    public void focusLost(FocusEvent event) {
        this.runInstrumentation(TerminalInstrumentationType.UNFOCUSED);
    }

    private void runInstrumentation(TerminalInstrumentationType instrumentationType) {
        this.service.instrumentTerminalContent(this.content, this.id, instrumentationType);
    }
}
