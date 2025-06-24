package io.fig.jetbrains.terminal;

import com.intellij.ui.content.Content;

public class TerminalStatus {

    private boolean focused;

    public TerminalStatus(Content content, boolean focused, int id) {
        this.focused = focused;
    }

    public boolean isFocused() {
        return this.focused;
    }

    public void setFocused(boolean focused) {
        this.focused = focused;
    }
}
