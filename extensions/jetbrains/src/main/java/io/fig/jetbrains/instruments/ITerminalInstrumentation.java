package io.fig.jetbrains.instruments;

import com.intellij.ui.content.Content;

public interface ITerminalInstrumentation {

    void instrumentTerminalContent(Content content, int id, TerminalInstrumentationType instrumentation);
}
