use super::*;

fn db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    crate::db::run_migrations(&conn).unwrap();
    conn.execute(
        "INSERT INTO subjects (id,name,colour,unit_level,subject_type,sort_order,created_at)
         VALUES (1,'Biology','#4BA97B','3_4','science',0,'2026-08-01T00:00:00Z'),
                (2,'Chemistry','#5B7FD4','3_4','science',1,'2026-08-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO topics (id,subject_id,name,sort_order) VALUES (10,1,'Genetics',0),(11,1,'Cells',1)",
        [],
    )
    .unwrap();
    conn
}

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, 16).unwrap()
}

/// Insert a card in a chosen state. `stability` is what decides strength.
#[allow(clippy::too_many_arguments)]
fn card(
    conn: &Connection,
    id: i64,
    subject: i64,
    topic: Option<i64>,
    state: &str,
    reps: i64,
    stability: Option<f64>,
    lapses: i64,
    due_on: Option<&str>,
) {
    conn.execute(
        "INSERT INTO cards (id,subject_id,topic_id,note_type,front,back,state,stability,
                            difficulty,due_on,reps,lapses,content_hash,created_at)
         VALUES (?1,?2,?3,'basic','f','b',?4,?5,5.0,?6,?7,?8,'h'||?1,'2026-08-01T00:00:00Z')",
        rusqlite::params![id, subject, topic, state, stability, due_on, reps, lapses],
    )
    .unwrap();
}

fn review(conn: &Connection, card_id: i64, date: &str, rating: i64) {
    conn.execute(
        "INSERT INTO review_log (item_type,item_id,subject_id,due_on,presented_at,rated_at,
                                 duration_ms,rating,local_date)
         VALUES ('card',?1,(SELECT subject_id FROM cards WHERE id=?1),?2,
                 ?2||'T09:00:00Z',?2||'T09:00:10Z',10000,?3,?2)",
        rusqlite::params![card_id, date, rating],
    )
    .unwrap();
}

// -- what counts as knowing something ---------------------------------------

/// The distinction the whole module exists for. A card answered right this
/// morning is not the same as a card you'll still have in three weeks, and
/// stability is what tells them apart.
#[test]
fn strength_comes_from_stability_not_from_being_answered_recently() {
    let conn = db();
    card(&conn, 1, 1, None, "new", 0, None, 0, None);            // never seen
    card(&conn, 2, 1, None, "review", 9, Some(3.0), 0, None);    // answered a lot, fragile
    card(&conn, 3, 1, None, "review", 2, Some(30.0), 0, None);   // answered twice, solid

    let m = &by_subject(&conn, today()).unwrap()[0];

    assert_eq!(m.strength.new, 1);
    assert_eq!(m.strength.learning, 1, "nine reps at 3 days' stability is not mastery");
    assert_eq!(m.strength.mastered, 1);
}

#[test]
fn a_card_still_in_learning_steps_is_never_mastered() {
    let conn = db();
    // Stability past the threshold, but not out of the learning steps yet.
    card(&conn, 1, 1, None, "learning", 3, Some(40.0), 0, None);
    card(&conn, 2, 1, None, "relearning", 9, Some(40.0), 2, None);

    let m = &by_subject(&conn, today()).unwrap()[0];

    assert_eq!(m.strength.mastered, 0);
    assert_eq!(m.strength.learning, 2);
}

#[test]
fn the_mastery_threshold_is_a_fortnight() {
    let conn = db();
    card(&conn, 1, 1, None, "review", 5, Some(13.9), 0, None);
    card(&conn, 2, 1, None, "review", 5, Some(14.0), 0, None);

    let m = &by_subject(&conn, today()).unwrap()[0];
    assert_eq!(m.strength.mastered, 1, "14 days is the line, and it is inclusive");
    assert_eq!(m.strength.learning, 1);
}

/// An empty deck is 0% known. Dividing by zero and calling the answer complete
/// is how a progress bar lies.
#[test]
fn a_subject_with_no_cards_is_zero_percent_not_a_hundred() {
    let conn = db();
    let all = by_subject(&conn, today()).unwrap();

    let chem = all.iter().find(|s| s.name == "Chemistry").unwrap();
    assert_eq!(chem.strength.total, 0);
    assert_eq!(chem.strength.mastery, 0.0);
    assert_eq!(chem.strength.next_due_on, None);
}

#[test]
fn mastery_is_the_share_of_cards_that_will_survive_a_fortnight() {
    let conn = db();
    for id in 1..=4 {
        card(&conn, id, 1, None, "review", 4, Some(30.0), 0, None);
    }
    for id in 5..=8 {
        card(&conn, id, 1, None, "review", 4, Some(2.0), 0, None);
    }

    let m = &by_subject(&conn, today()).unwrap()[0];
    assert_eq!(m.strength.mastery, 0.5);
}

/// Eight lapses means the card is wrong, not that you are. More repetitions
/// won't fix a card you never understood, so it is surfaced to be rewritten.
#[test]
fn a_card_forgotten_eight_times_is_flagged_as_a_leech() {
    let conn = db();
    card(&conn, 1, 1, None, "review", 20, Some(2.0), 7, None);
    card(&conn, 2, 1, None, "review", 20, Some(2.0), 8, None);

    assert_eq!(by_subject(&conn, today()).unwrap()[0].strength.leeches, 1);
}

// -- due counts and the next date -------------------------------------------

#[test]
fn overdue_cards_count_as_due_today() {
    let conn = db();
    card(&conn, 1, 1, None, "review", 3, Some(5.0), 0, Some("2026-08-10")); // overdue
    card(&conn, 2, 1, None, "review", 3, Some(5.0), 0, Some("2026-08-16")); // today
    card(&conn, 3, 1, None, "review", 3, Some(5.0), 0, Some("2026-08-20")); // later

    let m = &by_subject(&conn, today()).unwrap()[0];
    assert_eq!(m.strength.due_today, 2, "a card you missed is still waiting");
}

/// "Nothing until Thursday" is a useful thing for a deck to say; "0 due" alone
/// leaves you wondering whether it is finished or broken.
#[test]
fn a_deck_reports_when_it_next_comes_up() {
    let conn = db();
    card(&conn, 1, 1, None, "review", 3, Some(20.0), 0, Some("2026-08-19"));
    card(&conn, 2, 1, None, "review", 3, Some(20.0), 0, Some("2026-08-25"));

    assert_eq!(
        by_subject(&conn, today()).unwrap()[0].strength.next_due_on.as_deref(),
        Some("2026-08-19")
    );
}

#[test]
fn a_suspended_card_is_neither_due_nor_next_up() {
    let conn = db();
    card(&conn, 1, 1, None, "review", 3, Some(5.0), 0, Some("2026-08-16"));
    conn.execute("UPDATE cards SET suspended = 1 WHERE id = 1", []).unwrap();

    let m = &by_subject(&conn, today()).unwrap()[0];
    assert_eq!(m.strength.due_today, 0);
    assert_eq!(m.strength.next_due_on, None);
    assert_eq!(m.strength.suspended, 1);
    // Still counted in the total: a suspended card is part of the deck.
    assert_eq!(m.strength.total, 1);
}

// -- topics -----------------------------------------------------------------

#[test]
fn topics_break_a_subject_down_and_unfiled_cards_are_their_own_bucket() {
    let conn = db();
    card(&conn, 1, 1, Some(10), "review", 3, Some(30.0), 0, None);
    card(&conn, 2, 1, Some(10), "new", 0, None, 0, None);
    card(&conn, 3, 1, Some(11), "review", 3, Some(30.0), 0, None);
    card(&conn, 4, 1, None, "new", 0, None, 0, None);

    let topics = by_topic(&conn, 1, today()).unwrap();

    let genetics = topics.iter().find(|t| t.name == "Genetics").unwrap();
    assert_eq!(genetics.strength.total, 2);
    assert_eq!(genetics.strength.mastery, 0.5);

    // Most cards have no topic until a deck is organised. Hiding that bucket
    // would hide most of the deck.
    let unfiled = topics.iter().find(|t| t.topic_id.is_none()).unwrap();
    assert_eq!(unfiled.name, "Unfiled");
    assert_eq!(unfiled.strength.total, 1);

    assert_eq!(topics.last().unwrap().topic_id, None, "unfiled sorts last");
}

// -- recent accuracy --------------------------------------------------------

/// Hard means you knew it, slowly. Counting it as a miss would make honest
/// self-rating look like failure, and you would stop pressing it.
#[test]
fn only_again_counts_against_you() {
    let conn = db();
    card(&conn, 1, 1, None, "review", 4, Some(5.0), 0, None);

    review(&conn, 1, "2026-08-16", 1); // Again
    review(&conn, 1, "2026-08-16", 2); // Hard
    review(&conn, 1, "2026-08-16", 3); // Good
    review(&conn, 1, "2026-08-16", 4); // Easy

    let stats = deck(&conn, 1, None, today(), 7).unwrap();
    assert_eq!(stats.recent_reviews, 4);
    assert_eq!(stats.recent_accuracy, Some(0.75));
}

/// No reviews is not 0%. A bar at zero reads as "you got everything wrong"
/// when the truth is "you haven't started".
#[test]
fn a_deck_never_reviewed_has_no_accuracy_rather_than_zero() {
    let conn = db();
    card(&conn, 1, 1, None, "new", 0, None, 0, None);

    let stats = deck(&conn, 1, None, today(), 7).unwrap();
    assert_eq!(stats.recent_accuracy, None);
    assert_eq!(stats.recent_reviews, 0);
}

#[test]
fn the_heatmap_has_one_cell_per_day_including_days_you_did_nothing() {
    let conn = db();
    card(&conn, 1, 1, None, "review", 4, Some(5.0), 0, None);
    review(&conn, 1, "2026-08-14", 3);
    review(&conn, 1, "2026-08-16", 3);

    let stats = deck(&conn, 1, None, today(), 7).unwrap();

    assert_eq!(stats.recent.len(), 7);
    assert_eq!(stats.recent[0].date, "2026-08-10", "oldest first");
    assert_eq!(stats.recent[6].date, "2026-08-16");
    // A gap has to read as a gap rather than compressing the timeline.
    assert_eq!(stats.recent[5].reviews, 0);
    assert_eq!(stats.recent[4].reviews, 1);
}

#[test]
fn deck_stats_can_be_narrowed_to_one_topic() {
    let conn = db();
    card(&conn, 1, 1, Some(10), "review", 4, Some(30.0), 0, None);
    card(&conn, 2, 1, Some(11), "review", 4, Some(2.0), 0, None);
    review(&conn, 1, "2026-08-16", 3);
    review(&conn, 2, "2026-08-16", 1);

    let genetics = deck(&conn, 1, Some(10), today(), 7).unwrap();
    assert_eq!(genetics.strength.total, 1);
    assert_eq!(genetics.strength.mastered, 1);
    assert_eq!(genetics.recent_accuracy, Some(1.0), "only Genetics' own reviews");

    let whole = deck(&conn, 1, None, today(), 7).unwrap();
    assert_eq!(whole.recent_accuracy, Some(0.5));
}

#[test]
fn average_stability_ignores_cards_that_have_never_been_answered() {
    let conn = db();
    card(&conn, 1, 1, None, "review", 4, Some(10.0), 0, None);
    card(&conn, 2, 1, None, "review", 4, Some(20.0), 0, None);
    card(&conn, 3, 1, None, "new", 0, None, 0, None);

    let stats = deck(&conn, 1, None, today(), 7).unwrap();
    assert_eq!(stats.average_stability, Some(15.0));
}

#[test]
fn another_subjects_cards_never_leak_into_a_decks_numbers() {
    let conn = db();
    card(&conn, 1, 1, None, "review", 4, Some(30.0), 0, Some("2026-08-16"));
    card(&conn, 2, 2, None, "review", 4, Some(30.0), 0, Some("2026-08-16"));
    review(&conn, 2, "2026-08-16", 1);

    let bio = deck(&conn, 1, None, today(), 7).unwrap();
    assert_eq!(bio.strength.total, 1);
    assert_eq!(bio.recent_reviews, 0, "Chemistry's answers are not Biology's");
}
