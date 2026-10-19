import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'kanji_page.dart';
import 'rust/api.dart' hide search;
import 'rust/api.dart' as api show search;
import 'word_page.dart';

/// The number of words that the app asks for at a time.
const _pageSize = 50;

/// The app asks for more words when the user is within this number of rows of the end.
const _loadMoreMargin = 10;

/// The height of each row of the results, with room for the furigana of a headword.
const double _rowExtent = 70;

/// A row of the results that the user can pick.
sealed class SearchRow {
  const SearchRow();
}

class CharacterRow extends SearchRow {
  const CharacterRow(this.link);

  final CharacterLink link;
}

class WordRow extends SearchRow {
  const WordRow(this.summary);

  final WordSummary summary;
}

/// Opens the page of a row. A character row opens the page of the character, and the row of a
/// mutant opens the page of its base.
void openSearchRow(NavigatorState navigator, SearchRow row) {
  switch (row) {
    case CharacterRow(:final link):
      openKanjiPage(navigator, link.base ?? link.character);
    case WordRow(:final summary):
      openWordSummary(navigator, summary);
  }
}

/// Opens the page of a word. A word that is 1 kanji opens the page of that kanji with the entry
/// of the word expanded, not a word page.
void openWordSummary(NavigatorState navigator, WordSummary summary) {
  final kanji = summary.kanji;
  if (kanji != null) {
    openKanjiPage(navigator, kanji, highlightWordId: summary.id);
  } else {
    openWordPage(navigator, summary.id);
  }
}

/// The query of the search, its results, and the selected row. Each search screen shows the same
/// session, so the query and the results stay when the user leaves the search screen.
class SearchSession extends ChangeNotifier {
  SearchSession(this._dictionary) {
    text.addListener(_onTextChanged);
  }

  final Dictionary _dictionary;

  /// The text of the search field.
  final TextEditingController text = TextEditingController();

  /// Gives the search field focus. The search screen sets it.
  VoidCallback? _focusField;

  /// Gives the search field focus, for example when a page goes back to the search screen.
  void requestFocus() => _focusField?.call();

  String _text = '';
  String _query = '';
  List<SearchRow> _rows = const [];
  int _characterCount = 0;
  int _wordCount = 0;
  int _selected = 0;
  int _generation = 0;

  /// The number of the latest search. A result of an earlier search is discarded.
  int _searchNumber = 0;
  bool _hasMore = false;
  bool _loadingMore = false;
  Future<void> _searching = Future.value();

  /// The character rows, then the word rows.
  List<SearchRow> get rows => _rows;

  /// The index of the selected row.
  int get selected => _selected;

  SearchRow? get selectedRow => _rows.elementAtOrNull(_selected);

  /// Whether the field has a query.
  bool get hasQuery => _text.trim().isNotEmpty;

  /// A number that changes when the results of a new query arrive. The list then goes back to
  /// the top.
  int get generation => _generation;

  /// The text after the rows, for a query with no words.
  String? get footer {
    if (_generation == 0 || _wordCount > 0 || !hasQuery) return null;
    return _characterCount > 0 ? 'No words' : 'No results';
  }

  @override
  void dispose() {
    text.dispose();
    super.dispose();
  }

  /// The search runs on each change of the text, also on the text that an input method is still
  /// composing, with no delay.
  void _onTextChanged() {
    if (text.text == _text) return;
    _text = text.text;
    _searching = _search(_text);
    notifyListeners();
  }

  /// Gives the selected row when the results of the current text are in the list, so that Enter
  /// does not open a row of the query before. Gives `null` if the text changes in the meantime.
  Future<SearchRow?> selectedRowOfText() async {
    final current = _text;
    await _searching;
    return _text == current ? selectedRow : null;
  }

  Future<void> _search(String text) async {
    final query = text.trim();
    if (query == _query) return;
    _query = query;
    final number = ++_searchNumber;
    _loadingMore = false;
    final results = query.isEmpty
        ? const SearchResults(characters: [], words: [])
        : await _fetch(query, 0);
    if (number != _searchNumber) return;
    _rows = [
      for (final link in results.characters) CharacterRow(link),
      for (final summary in results.words) WordRow(summary),
    ];
    _characterCount = results.characters.length;
    _wordCount = results.words.length;
    _hasMore = results.words.length == _pageSize;
    _selected = 0;
    _generation++;
    notifyListeners();
  }

  /// Asks for the next words of the query, and adds them to the list. A page with fewer words
  /// than the app asked for is the last.
  Future<void> loadMore() async {
    if (!_hasMore || _loadingMore) return;
    _loadingMore = true;
    final number = _searchNumber;
    final results = await _fetch(_query, _wordCount);
    if (number != _searchNumber) return;
    _loadingMore = false;
    _rows = [..._rows, for (final summary in results.words) WordRow(summary)];
    _wordCount += results.words.length;
    _hasMore = results.words.length == _pageSize;
    notifyListeners();
  }

  /// Moves the selection up or down by [delta] rows.
  void moveSelection(int delta) {
    if (_rows.isEmpty) return;
    final selected = (_selected + delta).clamp(0, _rows.length - 1);
    if (selected == _selected) return;
    _selected = selected;
    if (_selected >= _rows.length - _loadMoreMargin) {
      loadMore();
    }
    notifyListeners();
  }

  Future<SearchResults> _fetch(String query, int offset) async {
    try {
      return await api.search(
        dictionary: _dictionary,
        query: query,
        offset: offset,
        limit: _pageSize,
      );
    } catch (error) {
      debugPrint('Search for "$query" failed: $error');
      return const SearchResults(characters: [], words: []);
    }
  }
}

/// Gives the search session to the screens below it.
class SearchScope extends InheritedWidget {
  const SearchScope({super.key, required this.session, required super.child});

  final SearchSession session;

  static SearchSession of(BuildContext context) {
    final scope = context.getInheritedWidgetOfExactType<SearchScope>();
    assert(scope != null, 'No SearchScope above this widget');
    return scope!.session;
  }

  @override
  bool updateShouldNotify(SearchScope oldWidget) =>
      session != oldWidget.session;
}

/// Goes back from a page to the search screen, the first screen of the app, and gives its field
/// focus. The history of pages starts again: the pages on top of the search screen close.
void openSearchScreen(BuildContext context) {
  Navigator.of(context).popUntil((route) => route.isFirst);
  SearchScope.of(context).requestFocus();
}

/// The search screen: the search field at the top, and the results below it. It is the first
/// screen of the app, and stays under the pages that it opens.
class SearchScreen extends StatefulWidget {
  const SearchScreen({super.key});

  @override
  State<SearchScreen> createState() => _SearchScreenState();
}

class _SearchScreenState extends State<SearchScreen> {
  late final SearchSession _session = SearchScope.of(context);
  late final FocusNode _focus = FocusNode(onKeyEvent: _onKey);
  final ScrollController _scroll = ScrollController();
  late int _generation = _session.generation;
  late int _selected = _session.selected;
  bool _hadFocusAtPointerDown = false;

  @override
  void initState() {
    super.initState();
    _session._focusField = _focus.requestFocus;
    _focus.addListener(_onFocusChanged);
    _scroll.addListener(_onScroll);
    _session.addListener(_onSessionChanged);
  }

  @override
  void dispose() {
    if (_session._focusField == _focus.requestFocus) {
      _session._focusField = null;
    }
    _session.removeListener(_onSessionChanged);
    _focus.dispose();
    _scroll.dispose();
    super.dispose();
  }

  /// When the field gets focus, the whole query is selected.
  void _onFocusChanged() {
    if (_focus.hasFocus) _selectAll();
  }

  void _selectAll() {
    _session.text.selection = TextSelection(
      baseOffset: 0,
      extentOffset: _session.text.text.length,
    );
  }

  /// A click that gives the field focus also puts the caret at the pointer, after the field has
  /// focus. So the whole query is selected again at the end of that click.
  void _onFieldTap() {
    if (!_hadFocusAtPointerDown) {
      _hadFocusAtPointerDown = true;
      _selectAll();
    }
  }

  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    if (event is KeyUpEvent) return KeyEventResult.ignored;
    // While an input method is composing, each key belongs to the input method: Enter commits
    // the text, and does not open a row.
    final composing = _session.text.value.composing;
    if (composing.isValid && !composing.isCollapsed) {
      return KeyEventResult.ignored;
    }
    switch (event.logicalKey) {
      case LogicalKeyboardKey.arrowDown:
        _session.moveSelection(1);
      case LogicalKeyboardKey.arrowUp:
        _session.moveSelection(-1);
      case LogicalKeyboardKey.enter || LogicalKeyboardKey.numpadEnter:
        _openSelected();
      default:
        return KeyEventResult.ignored;
    }
    return KeyEventResult.handled;
  }

  Future<void> _openSelected() async {
    final row = await _session.selectedRowOfText();
    if (row != null && mounted) _open(row);
  }

  /// Opens a row. The field loses focus, and the query stays in the field.
  void _open(SearchRow row) {
    _focus.unfocus();
    openSearchRow(Navigator.of(context), row);
  }

  /// Asks for more words when the last row in view is within the margin of the end of the list.
  void _onScroll() {
    final position = _scroll.position;
    final lastInView =
        (position.pixels + position.viewportDimension) / _rowExtent;
    if (_session.rows.length - lastInView < _loadMoreMargin) {
      _session.loadMore();
    }
  }

  void _onSessionChanged() {
    final isNewQuery = _session.generation != _generation;
    final hasMoved = _session.selected != _selected;
    _generation = _session.generation;
    _selected = _session.selected;
    setState(() {});
    if (!isNewQuery && !hasMoved) return;
    // The list has its new rows after the next frame.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_scroll.hasClients) return;
      if (isNewQuery) {
        _scroll.jumpTo(0);
      } else {
        _showSelected();
      }
    });
  }

  /// Scrolls only as far as it must to show the selected row.
  void _showSelected() {
    final position = _scroll.position;
    final top = _selected * _rowExtent;
    final bottom = top + _rowExtent;
    if (top < position.pixels) {
      _scroll.jumpTo(top);
    } else if (bottom > position.pixels + position.viewportDimension) {
      _scroll.jumpTo(bottom - position.viewportDimension);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(
        titleSpacing: 0,
        title: Listener(
          onPointerDown: (event) => _hadFocusAtPointerDown = _focus.hasFocus,
          child: TextField(
            controller: _session.text,
            focusNode: _focus,
            autofocus: true,
            onTap: _onFieldTap,
            style: const TextStyle(fontSize: 18),
            decoration: const InputDecoration(
              hintText: 'Search',
              prefixIcon: Icon(Icons.search),
              border: InputBorder.none,
            ),
          ),
        ),
      ),
      body: _session.hasQuery
          ? _buildResults(theme)
          : Center(
              child: Text(
                'Type kanji, kana, or romaji to search.',
                style: theme.textTheme.titleMedium?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ),
    );
  }

  Widget _buildResults(ThemeData theme) {
    final rows = _session.rows;
    final footer = _session.footer;
    return ListView.builder(
      controller: _scroll,
      itemExtent: _rowExtent,
      itemCount: rows.length + (footer == null ? 0 : 1),
      itemBuilder: (context, index) {
        if (index == rows.length) {
          return _Footer(text: footer!);
        }
        final row = rows[index];
        final isSelected = index == _session.selected;
        void onTap() => _open(row);
        return switch (row) {
          CharacterRow(:final link) => _CharacterTile(
            link: link,
            selected: isSelected,
            onTap: onTap,
          ),
          WordRow(:final summary) => _WordTile(
            summary: summary,
            selected: isSelected,
            onTap: onTap,
          ),
        };
      },
    );
  }
}

class _Footer extends StatelessWidget {
  const _Footer({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      child: Align(
        alignment: Alignment.centerLeft,
        child: Text(
          text,
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
      ),
    );
  }
}

/// The background of a row, and its tap.
class _Tile extends StatelessWidget {
  const _Tile({
    required this.selected,
    required this.onTap,
    required this.child,
  });

  final bool selected;
  final VoidCallback onTap;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Material(
      color: selected ? colors.secondaryContainer : Colors.transparent,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
          child: child,
        ),
      ),
    );
  }
}

/// A character row: the character, and the label "character". The row of a mutant has the label
/// "form of 水".
class _CharacterTile extends StatelessWidget {
  const _CharacterTile({
    required this.link,
    required this.selected,
    required this.onTap,
  });

  final CharacterLink link;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final base = link.base;
    return _Tile(
      selected: selected,
      onTap: onTap,
      child: Row(
        children: [
          Text(link.character, style: const TextStyle(fontSize: 26)),
          const SizedBox(width: 16),
          Text(
            base == null ? 'character' : 'form of $base',
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ],
      ),
    );
  }
}

/// A word row: the headword, the reading, and the gloss. A summary with a matched form shows it
/// after the reading, for example "matches 鳥渡".
class _WordTile extends StatelessWidget {
  const _WordTile({
    required this.summary,
    required this.selected,
    required this.onTap,
  });

  final WordSummary summary;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final muted = theme.textTheme.bodyMedium?.copyWith(
      color: theme.colorScheme.onSurfaceVariant,
    );
    final reading = summary.reading;
    final matchedForm = summary.matchedForm;
    return _Tile(
      selected: selected,
      onTap: onTap,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        mainAxisAlignment: MainAxisAlignment.center,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // A headword with furigana needs no reading after it.
          if (summary.furigana.isNotEmpty)
            Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                FuriganaText(parts: summary.furigana, fontSize: 18),
                if (matchedForm != null)
                  Flexible(
                    child: Text(
                      '  matches $matchedForm',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: muted?.copyWith(height: 1.6),
                    ),
                  ),
              ],
            )
          else
            Text.rich(
              TextSpan(
                children: [
                  TextSpan(
                    text: summary.headword,
                    style: const TextStyle(fontSize: 18),
                  ),
                  if (reading != null) TextSpan(text: '  $reading'),
                  if (matchedForm != null)
                    TextSpan(text: '  matches $matchedForm', style: muted),
                ],
              ),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(height: 1.2),
            ),
          Text(
            summary.gloss,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: muted?.copyWith(height: 1.2),
          ),
        ],
      ),
    );
  }
}
