// Charlie: Last Call. Code-only logic from the TypeScript engine.
// Room files still answer anything this script does not handle.

fn prompt() {
    "Restore, restart, or quit?"
}

fn die_auto(text) {
    say(text);
    say(prompt());
    mark_dead("autosave");
    handled();
}

fn die_named(text, checkpoint) {
    say(text);
    say(prompt());
    mark_dead(checkpoint);
    handled();
}

fn drip() {
    if flag("wet") {
        say("Charlie is dripping. He leaves a trail of water.");
    }
}

fn on_early() {
    if flag("ended") {
        say("The night is already over.");
        handled();
        return;
    }
    if verb() == "stand" {
        if flag("tied") {
            say("Charlie can't get his feet under him. His ankles are tied to the chair.");
        } else if flag("inPool") {
            say("Charlie is under the water.");
        } else {
            say("Charlie is standing.");
        }
        handled();
        return;
    }
    if verb() == "score" {
        say("Your score is " + score_now().to_string() + " of 168.");
        handled();
        return;
    }
    if verb() == "help" {
        say("Try look, talk, get, give, buy, play, put, wear, search, show, and go. F3 repeats. The alley is wet, and so are you.");
        handled();
        return;
    }
}

fn on_cutscene_command() {
    if verb() == "shout" || verb() == "attack" || verb() == "insult" {
        die_auto("You yell. Two suits look over the Buick, and the night gets very small. Charlie has learned a lesson he will not be retaining.");
        return;
    }
    say("Charlie stays very quiet. A wise man would keep it that way.");
    handled();
}

fn on_enter() {
    if !flag("live") {
        set_flag("live");
        face("n");
        return;
    }
    clear_flag("onDumpster");
    if flag("quietEnter") {
        clear_flag("quietEnter");
        keep_autosave();
        if flag("tied") {
            hold(true);
            teleport(114, 152);
            picture("tied");
            hide_actor("charlie");
            hide_actor("thug");
        }
        return;
    }
    let where = room();
    if where == "alley" && !flag("seenCutscene") {
        play_cutscene("alley-dana");
    } else if where == "alley" && flag("alleyChase") {
        say("Rain, and a thug remembering how his feet work. The dumpster. Then the ladder.");
    } else if where == "alley" {
        say("Rain still hammers the alley.");
    } else if where == "club" {
        say("Bass, smoke, and a mirror ball turning for almost nobody. The Purple Rhino is open.");
    } else if where == "bathroom" {
        say("You step into the gents. The storm is only a rumor through the vent.");
    } else if where == "rooftop" {
        say("Rain on the tar. Vents tick. A water tank keeps the skyline company, and a locked door leads down.");
    } else if where == "street" {
        say("The awning drips. Brass doors. A doorman who looks paid to be calm. Charlie has made it out. He thinks.");
    } else if where == "deck" {
        if flag("poolDone") {
            say("The pool deck is quiet. The balcony is empty. A maintenance shed sits by the water.");
        } else {
            say("Night on the pool deck. A balcony guard walks a flashlight across the boards. There is no dry place the beam will miss.");
        }
    } else if where == "hall" {
        say("The entrance hall is empty. The guard who was posted here has gone to check the pool.");
    } else if where == "kitchen" {
        say("A kitchen that has fed worse men. The pantry door is the interesting one. The open door on the left goes back to the hall.");
    } else if where == "hallway" {
        if flag("hallClear") {
            say("Smoke hangs in the hallway. The chair by the office door is empty.");
        } else {
            say("A hallway. A guard sleeps in a chair in front of Vic's office door. Close is a bad idea.");
        }
    } else if where == "office" {
        if flag("policeHere") {
            say("An officer stands in Vic's office. Dana is free. The drawer is open.");
        } else if flag("danaFree") {
            say("Vic's office. Dana is free. The desk phone is waiting.");
        } else {
            say("Vic's office. Dana is tied to a chair. A desk phone. A drawer.");
        }
    } else if where == "billiard" {
        if flag("thugDown") {
            say("The billiard room. The thug is still on the rug.");
        } else {
            say("The billiard room.");
        }
    } else if where == "bar" && !flag("heardThunder") {
        set_flag("heardThunder");
        say("Thunder rolls a long way off. Rain ticks the bar windows.");
    } else if where == "bar" {
        say("You step back into the Rusty Nail.");
    }
    if where == "alley" {
        if flag("alleyChase") {
            hide_actor("tank");
        } else {
            clear_pose("tank");
        }
    } else if where == "club" {
        if flag("clubChase") {
            hide_actor("goon");
        } else {
            clear_pose("goon");
        }
    } else if where == "hallway" {
        if flag("hallClear") {
            picture("clear");
            hide_actor("guard");
        } else {
            picture("");
            clear_pose("guard");
        }
    } else if where == "office" {
        if flag("policeHere") {
            picture("police");
            hide_actor("dana");
        } else if flag("danaFree") {
            picture("");
            hide_actor("dana");
        } else {
            picture("");
            clear_pose("dana");
        }
    } else if where == "billiard" && flag("thugDown") {
        pose_at("thug", "thug_ko", 230, 195);
    } else if where == "billiard" && !flag("tied") {
        clear_pose("thug");
    }
    drip();
    if flag("tied") {
        hold(true);
        teleport(114, 152);
        picture("tied");
        hide_actor("charlie");
        hide_actor("thug");
    } else {
        hold(false);
    }
    if flag("dry") {
        costume("dry");
    } else {
        costume("default");
    }
}

fn plate_handoff() {
    if (verb() == "give" || verb() == "show" || verb() == "use") && noun() == "note" {
        return second() == "" || second() == "officer";
    }
    if verb() == "talk" && noun() == "officer" && second() == "note" {
        return true;
    }
    false
}

fn books_free() {
    if verb() == "look" && noun() == "books" {
        return true;
    }
    if (verb() == "give" || verb() == "show") && noun() == "books" {
        return true;
    }
    false
}

fn end_good() {
    if !already("give-plate") {
        score("give-plate", 5);
    }
    let total = score_now() + (if already("give-plate") { 0 } else { 5 });
    set_flag("ended");
    set_flag("goodEnd");
    say("Charlie gives the officer the napkin. NY plate KTL-4471. The officer is already on the radio.");
    say("They catch Vic on the road out. The car matches the napkin.");
    say("Dana turns state's witness. The true books do the rest.");
    say("Her mother is all right. She is in the hospital, and she is recovering. The bills are no longer Vic's.");
    say("Later, the Rusty Nail. Sal sets down a glass. Charlie nurses it. Nothing happens. He likes it that way.");
    say("Charlie: Last Call.");
    say("Score: " + total.to_string() + " of 168.");
    finish("good");
    handled();
}

fn end_bad() {
    set_flag("ended");
    set_flag("badEnd");
    say("The officer writes down a house and a dock.");
    say("The napkin stays pocketed, or the moment passes. Either way the radio never gets the plate.");
    say("Vic is gone. The road eats the taillights.");
    say("Every set of headlights after that could be his. Charlie lives like a man who checks the street before he checks the drink.");
    say("Score: " + score_now().to_string() + " of 168.");
    finish("bad");
    handled();
}

fn police() {
    if !flag("policeHere") || flag("ended") {
        return false;
    }
    if verb() == "no" || (verb() == "talk" && (noun() == "no" || second() == "no")) {
        end_bad();
        return true;
    }
    if plate_handoff() {
        if has("note") {
            end_good();
        } else {
            end_bad();
        }
        return true;
    }
    if books_free() {
        return false;
    }
    if num("police") + 1 > 4 {
        end_bad();
        return true;
    }
    set_var("police", num("police") + 1);
    false
}

fn going() {
    verb() == "go"
}

fn hall_death() {
    "A guard is posted in the entrance hall. He was not expecting company. Charlie doesn't get a second look."
}

fn wet_death() {
    "Charlie is still dripping. Wet footprints cross the entrance hall. The guard, back from the pool, looks at the trail and then at the clothes. He does not miss it."
}

fn nap_death() {
    "Charlie steps too close. The nap ends. The guard's hand is faster than the apology."
}

fn beam_death() {
    "The flashlight finds Charlie on the dry deck. There is no place to stand that the beam will miss. A shout, then hands."
}

fn surface_death() {
    "Charlie comes up too soon. The beam is still on the water. The balcony sees a face, and the night ends."
}

fn go_hall_from_billiard() {
    if !flag("poolDone") {
        die_auto(hall_death());
        return;
    }
    if flag("wet") {
        die_auto(wet_death());
        return;
    }
    goto_room("hall", "fromBilliard");
    handled();
}

fn billiard_travel() {
    if !going() {
        return false;
    }
    if flag("tied") {
        say("Charlie's ankles are still tied to the chair.");
        handled();
        return true;
    }
    if noun() == "out" || noun() == "pool" || noun() == "window" || noun() == "deck" || noun() == "water" {
        goto_room("deck", "fromBilliard");
        handled();
        return true;
    }
    if noun() == "door" || noun() == "hall" || noun() == "lobby" {
        go_hall_from_billiard();
        return true;
    }
    if noun() == "hallway" {
        say("The hallway is past the entrance hall. One room at a time.");
        handled();
        return true;
    }
    if noun() == "kitchen" {
        say("The kitchen is off the entrance hall.");
        handled();
        return true;
    }
    if noun() == "office" {
        say("Vic's office is not this door.");
        handled();
        return true;
    }
    say("The way out is the door to the hall, or the windows onto the pool deck.");
    handled();
    true
}

fn surfing() {
    if verb() == "surface" {
        return true;
    }
    if verb() == "jump" && noun() == "out" {
        return true;
    }
    if verb() == "get" && noun() == "out" {
        return true;
    }
    if flag("inPool") && going() && (noun() == "out" || noun() == "lobby" || noun() == "back" || noun() == "billiard" || noun() == "door") {
        return true;
    }
    false
}

fn deck_look() {
    if noun() == "" || noun() == "around" || noun() == "room" {
        if flag("poolDone") {
            let quiet = "The pool deck is quiet. The balcony is empty. A maintenance shed sits by the water, and the beam is gone.";
            if flag("wet") {
                say(quiet + " Charlie is dripping. He leaves a trail of water.");
            } else {
                say(quiet);
            }
        } else if flag("inPool") {
            say("Black water. A flashlight walks the deck above him. Coming up now would be the end of it.");
        } else {
            say("Night on the pool deck. A balcony guard walks a flashlight across the boards. There is no dry place the beam will miss. The pool might.");
        }
        return true;
    }
    if noun() == "guard" {
        if flag("poolDone") {
            say("The balcony is empty. He went to check on the raccoons, and he has not come back.");
        } else {
            say("A guard on the balcony, and a flashlight that does not sit still. The deck is his.");
        }
        return true;
    }
    if noun() == "pool" || noun() == "water" {
        if flag("poolDone") {
            say("The pool is black and quiet. It already did the job.");
        } else {
            say("Dark water, big enough to hide a man if he stays under.");
        }
        return true;
    }
    if noun() == "shed" {
        if flag("poolDone") {
            say("A maintenance shed by the water. Chlorine tablets, and not much else Charlie wants.");
        } else {
            say("A shed by the water. The beam owns the walk to it.");
        }
        return true;
    }
    false
}

fn deck_cmd() {
    if verb() == "look" {
        if deck_look() {
            handled();
            return true;
        }
        return false;
    }
    if verb() == "dive" || (verb() == "jump" && (noun() == "pool" || noun() == "water")) || (going() && (noun() == "pool" || noun() == "water")) {
        if flag("poolDone") {
            say("The guard is already gone. The pool has done its job.");
        } else if flag("inPool") {
            say("Charlie is already under. The next trick is staying there.");
        } else {
            set_flag("inPool");
            say("Charlie slips into the pool. The water is black and cold. Above him, the beam keeps walking the deck.");
        }
        handled();
        return true;
    }
    if verb() == "hold" && (noun() == "" || noun() == "breath") {
        if flag("poolDone") {
            say("The guard already bought the raccoon story. Charlie can breathe.");
        } else if !flag("inPool") {
            say("The pool is the cover. The deck is not. Charlie needs to be in the water first.");
        } else {
            set_flag("poolDone");
            set_flag("wet");
            clear_flag("inPool");
            clear_flag("dry");
            score("pool-breath", 5);
            sfx("score");
            say("Charlie stays under and holds his breath. The flashlight sweeps the deck, then the water.");
            say("A voice from the balcony: \"Must be the raccoons again.\"");
            say("The beam moves off. Footsteps leave. Charlie comes up along the edge, dripping, and the deck is his.");
        }
        handled();
        return true;
    }
    if surfing() || (flag("inPool") && verb() == "wait") {
        if !flag("inPool") {
            say("Charlie is not in the water.");
            handled();
            return true;
        }
        if flag("poolDone") {
            clear_flag("inPool");
            say("Charlie is already out of the pool.");
            handled();
            return true;
        }
        die_auto(surface_death());
        return true;
    }
    if going() && (noun() == "lobby" || noun() == "back" || noun() == "billiard" || noun() == "door" || noun() == "window") {
        if flag("inPool") && !flag("poolDone") {
            die_auto(surface_death());
            return true;
        }
        goto_room("billiard", "fromDeck");
        handled();
        return true;
    }
    if verb() == "get" && (noun() == "chlorine" || noun() == "shed" || noun() == "tablets") {
        if flag("inPool") && !flag("poolDone") {
            say("He can't reach the shed from under the water.");
            handled();
            return true;
        }
        if !flag("poolDone") {
            die_auto(beam_death());
            return true;
        }
        if has("chlorine") || has("bomb") {
            say("Charlie already took the chlorine tablets.");
        } else {
            take_item("chlorine");
            score("get-chlorine", 3);
            sfx("score");
            say("Charlie takes a tin of chlorine tablets from the shed. The rest of the jars he leaves alone.");
        }
        handled();
        return true;
    }
    if flag("poolDone") {
        if verb() == "wait" {
            say("The deck stays quiet. The balcony stays empty.");
            handled();
            return true;
        }
        return false;
    }
    if flag("inPool") {
        die_auto(surface_death());
        return true;
    }
    die_auto(beam_death());
    true
}

fn hall_cmd() {
    if verb() == "look" && (noun() == "" || noun() == "around" || noun() == "room") {
        say("The entrance hall of Vic's lakehouse. Dark wood, a coat rack, and no guard. He went to check the pool. The kitchen door is open on the right.");
        if flag("wet") {
            say("Charlie is dripping. He leaves a trail of water.");
        }
        handled();
        return true;
    }
    if verb() == "look" && noun() == "guard" {
        say("The guard who was posted here has gone out to the pool. The hall is clear.");
        handled();
        return true;
    }
    if !going() {
        return false;
    }
    if noun() == "kitchen" {
        goto_room("kitchen", "default");
        handled();
        return true;
    }
    if noun() == "hallway" {
        goto_room("hallway", "fromHall");
        handled();
        return true;
    }
    if noun() == "back" || noun() == "billiard" || noun() == "door" || noun() == "out" || noun() == "lobby" {
        goto_room("billiard", "fromHall");
        handled();
        return true;
    }
    if noun() == "office" {
        say("Vic's office is down the hallway, and a sleeping man is in the way.");
        handled();
        return true;
    }
    if noun() == "deck" || noun() == "pool" {
        say("The pool is back through the billiard room.");
        handled();
        return true;
    }
    say("The kitchen is one way. The hallway is the other. The billiard room is behind him.");
    handled();
    true
}

fn making_bomb() {
    if verb() == "make" || verb() == "combine" {
        return true;
    }
    if verb() == "use" && (noun() == "chlorine" || second() == "chlorine") {
        return true;
    }
    false
}

fn kitchen_cmd() {
    if (verb() == "look" || verb() == "open") && (noun() == "pantry" || noun() == "jar" || (verb() == "open" && noun() == "door")) {
        say("Mr. Hadley always said never to mix these... Sorry, Mr. Hadley.");
        handled();
        return true;
    }
    if verb() == "look" && (noun() == "" || noun() == "around" || noun() == "room") {
        say("A kitchen that has fed worse men. The pantry door is the interesting one. The open door on the left goes back to the hall.");
        if flag("wet") {
            say("Charlie is dripping. He leaves a trail of water.");
        }
        handled();
        return true;
    }
    if making_bomb() {
        if has("bomb") {
            say("One smoke bomb is enough.");
        } else if !has("chlorine") {
            say("Charlie needs the chlorine tablets from the shed before he can do what Mr. Hadley forbade.");
        } else {
            give_item("chlorine");
            take_item("bomb");
            score("make-bomb", 4);
            sfx("score");
            say("Charlie does what Mr. Hadley said never to do. He does not look too closely at which jar. The chlorine tablets go in. What he cups in his hands is a smoke bomb, and that is all he lets himself know.");
        }
        handled();
        return true;
    }
    if !going() {
        return false;
    }
    if noun() == "hall" || noun() == "back" || noun() == "door" || noun() == "out" || noun() == "lobby" {
        goto_room("hall", "fromKitchen");
        handled();
        return true;
    }
    say("The entrance hall is the way back.");
    handled();
    true
}

fn bomb_throw() {
    if noun() != "bomb" && second() != "bomb" {
        return false;
    }
    verb() == "drop" || verb() == "use"
}

fn hallway_cmd() {
    if bomb_throw() {
        if flag("hallClear") {
            say("The guard is already gone, coughing his way somewhere else.");
        } else if !has("bomb") {
            say("Charlie has nothing that will move that guard from a distance.");
        } else {
            give_item("bomb");
            set_flag("hallClear");
            score("smoke-guard", 5);
            sfx("score");
            say("Charlie throws the smoke bomb down the hall. It pops, and the corridor fills.");
            say("The guard staggers up, coughing and spluttering, and blunders off the other way.");
            say("The chair is empty. Vic's office door is clear.");
        }
        handled();
        return true;
    }
    if verb() == "look" && (noun() == "" || noun() == "around" || noun() == "room" || noun() == "guard" || noun() == "door") {
        if flag("hallClear") {
            say("Smoke hangs in the hallway. The chair by Vic's office door is empty.");
            if flag("wet") {
                say("Charlie is dripping. He leaves a trail of water.");
            }
        } else if noun() == "door" {
            say("Vic's office door. A sleeping guard in a chair is the lock. Close is how the nap ends.");
        } else {
            say("A hallway in Vic's lakehouse. A guard sleeps in a chair in front of the office door. Charlie is far enough back to stay a rumor.");
            if flag("wet") {
                say("Charlie is dripping. He leaves a trail of water.");
            }
        }
        handled();
        return true;
    }
    if (going() && (noun() == "office" || noun() == "door")) || (verb() == "open" && (noun() == "door" || noun() == "office")) {
        if !flag("hallClear") {
            die_auto(nap_death());
            return true;
        }
        goto_room("office", "default");
        handled();
        return true;
    }
    if !going() {
        return false;
    }
    if noun() == "hall" || noun() == "back" || noun() == "lobby" || noun() == "out" {
        goto_room("hall", "fromHallway");
        handled();
        return true;
    }
    say("Back is the entrance hall. Ahead is Vic's office, and the man asleep in front of it.");
    handled();
    true
}

fn untying() {
    if verb() == "untie" && (noun() == "" || noun() == "woman" || noun() == "rope" || noun() == "chair") {
        return true;
    }
    if verb() == "use" && noun() == "knife" && (second() == "" || second() == "rope" || second() == "woman" || second() == "chair") {
        return true;
    }
    false
}

fn calling() {
    if verb() == "call" && (noun() == "" || noun() == "officer" || noun() == "phone") {
        return true;
    }
    if verb() == "use" && noun() == "phone" && (second() == "" || second() == "officer") {
        return true;
    }
    false
}

fn searching_desk() {
    if verb() == "search" && (noun() == "desk" || noun() == "books" || noun() == "drawer" || noun() == "") {
        return true;
    }
    if verb() == "open" && (noun() == "desk" || noun() == "books" || noun() == "drawer") {
        return true;
    }
    if verb() == "get" && noun() == "books" {
        return true;
    }
    false
}

fn office_cmd() {
    if untying() {
        if flag("danaFree") {
            say("Dana's hands are already free.");
        } else {
            set_flag("danaFree");
            score("untie-dana", 4);
            sfx("score");
            say("Dana's wrists come free. The pencil is still in her bun. She keeps her voice down.");
            say("\"I was cooking Vic's books,\" she says. \"Not for the skim. For my mother.\"");
            say("\"Her hospital bills. Vic paid them, then he owned them. If I stopped, she didn't get a bed. If I talked, she didn't get to stay in one.\"");
            say("\"I kept a second set. The true books. I was going to get us both out.\"");
        }
        handled();
        return true;
    }
    if calling() {
        if !flag("danaFree") {
            say("Dana is still tied. The phone can wait until her hands are free.");
        } else if flag("policeCalled") {
            say("The police are already on the way.");
        } else {
            set_flag("policeCalled");
            score("call-police", 3);
            sfx("score");
            say("Charlie lifts the desk phone and tells them where Vic's lakehouse is. He keeps his voice down. They are on the way.");
        }
        handled();
        return true;
    }
    if searching_desk() {
        if !flag("danaFree") {
            say("Dana first. The drawer can wait.");
        } else if !flag("policeCalled") {
            say("Not yet. If he opens that drawer before the police are coming, Vic is the one who finds him.");
        } else if has("books") || flag("vicFled") {
            say("The true books are already out of the drawer.");
        } else {
            take_item("books");
            set_flag("vicFled");
            set_flag("policeHere");
            set_var("police", 0);
            score("get-books", 4);
            sfx("score");
            say("The drawer holds a second set of account books. The numbers in them are the true ones.");
            say("The door flies open. Vic stands in it, a gun already up. He does not fire.");
            say("Sirens cut across the water. He curses, low and ugly, and runs.");
            say("An engine catches. Tires spit gravel. The car tears off down the road.");
            say("An officer comes in through the door Charlie left open. The gun is gone. Vic is not.");
        }
        handled();
        return true;
    }
    if verb() == "look" && noun() == "books" && has("books") {
        score("read-books", 2);
        sfx("score");
        say("Charlie opens the true books. The difference is the skim, and Dana's handwriting is on the honest pages.");
        handled();
        return true;
    }
    if (verb() == "give" || verb() == "show") && noun() == "books" {
        if !has("books") {
            say("Charlie doesn't have the account books.");
        } else if !flag("policeHere") {
            say("There is nobody official to hand them to yet.");
        } else {
            give_item("books");
            score("give-books", 3);
            sfx("score");
            say("Charlie gives the officer the true books. The officer takes them like they matter.");
        }
        handled();
        return true;
    }
    if verb() == "look" && (noun() == "" || noun() == "around" || noun() == "room") {
        if flag("policeHere") {
            say("Vic's office. An officer stands where the gun was. Dana is free. The drawer is open.");
        } else if flag("danaFree") {
            say("Vic's office. Dana is unbound, voice still low. A desk phone. A drawer Charlie has not earned yet.");
        } else {
            say("Vic's office. Dana is tied to a chair, pencil in her bun, red cardigan crooked. A desk phone. A drawer.");
        }
        if flag("wet") {
            say("Charlie is dripping. He leaves a trail of water.");
        }
        handled();
        return true;
    }
    if verb() == "look" && noun() == "woman" {
        if flag("danaFree") {
            say("Dana rubs her wrists. The pencil stays in the bun. She is not running. She is waiting to see if this works.");
        } else {
            say("Dana Whitlock, tied to a chair. The pencil is still in her bun. The red cardigan is crooked. Her eyes find Charlie.");
        }
        handled();
        return true;
    }
    if verb() == "look" && (noun() == "phone" || noun() == "desk") {
        say("A desk phone, and a drawer under the blotter. Order matters.");
        handled();
        return true;
    }
    if verb() == "look" && noun() == "officer" && flag("policeHere") {
        say("A uniform, a notebook, and the patience of a man who heard the siren from the inside. He is waiting on something he can use.");
        handled();
        return true;
    }
    if verb() == "talk" && noun() == "woman" {
        if !flag("danaFree") {
            say("She shakes her head, small. Not while the ropes are on. Not while the house can hear.");
        } else {
            say("\"The phone,\" she says, barely. \"Then the drawer. The true books are in it.\"");
        }
        handled();
        return true;
    }
    if !going() {
        return false;
    }
    if noun() == "hallway" || noun() == "back" || noun() == "door" || noun() == "out" || noun() == "hall" {
        goto_room("hallway", "fromOffice");
        handled();
        return true;
    }
    say("The hallway is the way back.");
    handled();
    true
}

fn books_anywhere() {
    if verb() == "look" && noun() == "books" {
        if !has("books") {
            say("Charlie doesn't have the account books.");
        } else {
            score("read-books", 2);
            sfx("score");
            say("Charlie opens the true books. The difference is the skim, and Dana's handwriting is on the honest pages.");
        }
        handled();
        return true;
    }
    if (verb() == "give" || verb() == "show") && noun() == "books" {
        if !has("books") {
            say("Charlie doesn't have the account books.");
        } else if !flag("policeHere") {
            say("There is nobody official to hand them to yet.");
        } else {
            give_item("books");
            score("give-books", 3);
            sfx("score");
            say("Charlie gives the officer the true books. The officer takes them like they matter.");
        }
        handled();
        return true;
    }
    false
}

fn away() {
    let t = num("thug");
    ((t % 3) + 3) % 3 == 2
}

fn burning() {
    if verb() == "burn" {
        return noun() == "" || noun() == "rope" || noun() == "matchbook" || noun() == "chair";
    }
    if verb() != "use" {
        return false;
    }
    if noun() == "matchbook" && (second() == "" || second() == "rope") {
        return true;
    }
    if noun() == "rope" && (second() == "" || second() == "matchbook") {
        return true;
    }
    false
}

fn loot_noun() {
    noun() == "knife" || noun() == "chips" || noun() == "handkerchief"
}

fn loot_look(kind) {
    if kind == "knife" {
        return "A short, cheap knife. The kind that lives in a coat pocket.";
    }
    if kind == "chips" {
        return "A few chips from the Golden Cow Casino. Someone at Vic's lakehouse has been spending time there.";
    }
    "A handkerchief, embroidered with the name Vinny."
}

fn pocket_line() {
    let underwear = if flag("dry") { " He is down to his underwear." } else { "" };
    let waiting = "";
    if !has("knife") {
        waiting = "a knife";
    }
    if !has("chips") {
        if waiting == "" {
            waiting = "a few casino chips";
        } else {
            waiting = waiting + ", a few casino chips";
        }
    }
    if !has("handkerchief") {
        if waiting == "" {
            waiting = "a handkerchief";
        } else {
            waiting = waiting + ", a handkerchief";
        }
    }
    if waiting == "" {
        return "Charlie has already gone through the thug's pockets." + underwear;
    }
    "The thug is out cold. His pockets hold " + waiting + "." + underwear
}

fn clothes_cmd() {
    if verb() == "change" && (noun() == "" || noun() == "clothes") {
        return true;
    }
    (verb() == "get" || verb() == "wear" || verb() == "use") && noun() == "clothes"
}

fn billiard_cmd() {
    if burning() {
        if !flag("vicLeft") {
            say("Not yet. The thug is watching, and the room still feels like it is waiting on someone.");
        } else if flag("handsFree") {
            say("The rope is already burned through. That was the last match, and it is gone.");
        } else if !has("matchbook") {
            if flag("floorBook") {
                if away() {
                    say("The matchbook is on the floor by the chair. Scoot over and get it while his back is turned.");
                } else {
                    say("The matchbook is on the floor by the chair. The thug is looking. Charlie leaves it there.");
                }
            } else {
                say("Charlie has no matches. The rope stays tied.");
            }
        } else if !away() {
            die_auto("The match flares behind the chair. The thug sees it. He does not ask what Charlie was lighting. The night ends there.");
            return true;
        } else {
            give_item("matchbook");
            set_flag("handsFree");
            clear_flag("floorBook");
            set_var("thug", num("thug") + 1);
            score("burn-rope", 5);
            sfx("score");
            say("Charlie strikes a match behind his back. The flame licks his wrists and he winces, but he keeps his hands behind him so the thug doesn't notice.");
            say("The rope smoulders and stinks, but the thug's cigarette smoke covers it. He doesn't notice a thing.");
            say("That was the last match.");
        }
        handled();
        return true;
    }
    if verb() == "wait" {
        if !flag("vicLeft") {
            say("Charlie waits. The thug smokes, and he does not look away.");
        } else if flag("thugDown") {
            say("The thug stays on the rug. He is done watching anything.");
        } else {
            let next = num("thug") + 1;
            set_var("thug", next);
            if ((next % 3) + 3) % 3 == 2 {
                say("The thug turns to the window and flicks ash. His back is to the chair. This is the moment.");
            } else {
                say("The thug turns back toward the chair. He is watching Charlie again.");
            }
        }
        handled();
        return true;
    }
    if verb() == "look" && (noun() == "" || noun() == "around" || noun() == "room") {
        let text = "A large billiard room in Vic's lakehouse. Dark wood, a pool table, a bar cart, antlers on the wall, tall windows onto the night lake, and one door.";
        if flag("thugDown") {
            text = text + " The thug is out cold on the rug.";
        } else if flag("vicLeft") && away() {
            text = text + " The thug is at the window, back turned, smoking.";
        } else {
            text = text + " A thug leans nearby, feet planted, smoking.";
        }
        if flag("handsFree") {
            text = text + " The rope on Charlie's wrists is burned through. He keeps his hands behind him.";
        } else {
            text = text + " Charlie's hands are tied behind his back.";
        }
        if !flag("tied") {
            text = text + " His ankles are free. The door leads to the entrance hall. The windows open onto the pool deck.";
        }
        if flag("floorBook") && !has("matchbook") && !flag("handsFree") {
            text = text + " A matchbook lies on the floor by the chair.";
        }
        if flag("handsFree") && !has("ball") && !flag("thugDown") {
            text = text + " A billiard ball sits within reach of the chair.";
        }
        if flag("dry") {
            text = text + " The thug is down to his underwear.";
        }
        if flag("wet") {
            text = text + " Charlie is dripping. He leaves a trail of water.";
        }
        say(text);
        handled();
        return true;
    }
    if verb() == "look" && noun() == "clothes" {
        if !flag("thugDown") {
            say("The thug is still in them.");
        } else if flag("dry") {
            say("Charlie is wearing the dry set. What the thug has left is underwear.");
        } else {
            say("A dry suit on a man who cannot object. Charlie's own clothes are another story.");
        }
        handled();
        return true;
    }
    if clothes_cmd() {
        if !flag("thugDown") {
            say("The thug is still wearing them, and he is still awake.");
        } else if flag("tied") {
            say("Charlie's ankles are still tied to the chair. The wardrobe can wait.");
        } else if flag("dry") && !flag("wet") {
            say("Charlie is already in the thug's dry clothes. They are not an improvement. They are drier.");
        } else if !flag("wet") {
            say("Charlie is dry enough. The thug can keep his clothes until the pool changes that.");
        } else {
            set_flag("dry");
            clear_flag("wet");
            score("thug-clothes", 3);
            sfx("score");
            costume("dry");
            say("Charlie takes the thug's dry clothes and puts them on. The thug keeps the underwear, which is more dignity than the night has offered anyone. Charlie stops dripping. The suit he leaves behind could water a fern.");
        }
        handled();
        return true;
    }
    if verb() == "look" && noun() == "guard" {
        if flag("thugDown") {
            say(pocket_line());
        } else if flag("vicLeft") && away() {
            say("The thug has his back to the chair, smoking at the window. He is not watching Charlie's hands.");
        } else {
            say("A thug leans nearby, feet planted, watching Charlie. Smoke curls off his cigarette. He will notice a match.");
        }
        handled();
        return true;
    }
    if verb() == "look" && noun() == "window" {
        if flag("thugDown") {
            say("The windows show the night lake. The thug is not looking at them anymore.");
        } else if !flag("vicLeft") {
            say("Tall windows onto the night lake. Trees, black water, and the dark. The thug is not looking through them. He is looking at Charlie.");
        } else if away() {
            say("The thug is at the glass, back turned, flicking ash. If Charlie is going to do something with his hands, it is now.");
        } else {
            say("Tall windows onto the night lake. The thug keeps turning that way to smoke. When his back is to the chair, Charlie has a moment.");
        }
        handled();
        return true;
    }
    if verb() == "look" && noun() == "door" {
        if flag("tied") {
            say("One door. It is shut. Charlie is not walking anywhere with his ankles tied.");
        } else {
            say("The door leads into the entrance hall. The windows at the far side open onto the pool deck.");
        }
        handled();
        return true;
    }
    if verb() == "look" && noun() == "woman" {
        say("Dana isn't in this room. They took her somewhere else. The pencil and the red cardigan went with her.");
        handled();
        return true;
    }
    if verb() == "look" && noun() == "matchbook" {
        if has("matchbook") {
            return false;
        }
        if flag("handsFree") {
            say("That was the last match. The book is gone.");
        } else if flag("floorBook") {
            say("A matchbook lies on the floor by the chair. The cover still says to ask for Vic.");
        } else {
            say("Charlie has no matchbook.");
        }
        handled();
        return true;
    }
    if verb() == "look" && loot_noun() {
        if has(noun()) {
            say(loot_look(noun()));
        } else if flag("thugDown") {
            say("It's still on the thug. " + loot_look(noun()));
        } else {
            say("The thug is still on his feet.");
        }
        handled();
        return true;
    }
    if verb() == "talk" && (noun() == "guard" || noun() == "vic") {
        if noun() == "vic" {
            if flag("vicLeft") {
                say("Vic is gone. The murmur stayed in the room.");
            } else {
                say("Vic isn't taking questions.");
            }
        } else if flag("thugDown") {
            say("The thug is out cold. He has nothing to say.");
        } else {
            say("The thug exhales smoke. He is not here for conversation.");
        }
        handled();
        return true;
    }
    if verb() == "get" && (noun() == "matchbook" || noun() == "chair") && flag("floorBook") && !has("matchbook") {
        if !flag("vicLeft") {
            say("The chair would squeak, and everyone in the room would hear it.");
        } else if !away() {
            say("The chair squeaks on the wood. The thug looks over. Charlie sits very still.");
        } else {
            take_item("matchbook");
            clear_flag("floorBook");
            set_var("thug", num("thug") + 1);
            say("Charlie scoots the chair, quiet as he can, and closes a hand on the matchbook.");
        }
        handled();
        return true;
    }
    if verb() == "get" && noun() == "matchbook" {
        if has("matchbook") {
            say("Charlie already has the matchbook.");
        } else if flag("handsFree") {
            say("That was the last match. The book is gone.");
        } else {
            say("There is no matchbook here.");
        }
        handled();
        return true;
    }
    if verb() == "get" && noun() == "ball" {
        if has("ball") {
            say("Charlie already has the billiard ball.");
        } else if !flag("handsFree") {
            say("Charlie's hands are still tied behind his back.");
        } else if !flag("thugDown") && !away() {
            say("The thug is looking right at him. Charlie leaves the ball where it is.");
        } else {
            take_item("ball");
            if !flag("thugDown") {
                set_var("thug", num("thug") + 1);
            }
            score("get-ball", 3);
            sfx("score");
            say("Charlie takes the billiard ball from the edge of the table, hands still low, behind the chair.");
        }
        handled();
        return true;
    }
    if (verb() == "drop" || verb() == "use") && noun() == "ball" {
        if second() != "" && second() != "guard" {
            say("That is not what the ball is for.");
        } else if !flag("handsFree") {
            say("Charlie's hands are behind his back. The ball stays where it is.");
        } else if !has("ball") {
            say("Charlie doesn't have the billiard ball.");
        } else if flag("thugDown") {
            say("The thug is already out cold.");
        } else {
            give_item("ball");
            set_flag("thugDown");
            clear_flag("tied");
            hold(false);
            teleport(140, 200);
            face("s");
            picture("");
            show_actor("charlie");
            pose_at("thug", "thug_ko", 230, 195);
            score("bean-thug", 4);
            sfx("score");
            say("All those years of Little League finally paid off.");
            say("With the thug down, Charlie gets the rope off his ankles. The chair is done with him.");
        }
        handled();
        return true;
    }
    if verb() == "attack" && noun() == "guard" {
        if flag("thugDown") {
            say("The thug is already out cold.");
        } else if flag("handsFree") && has("ball") {
            say("Fists are a bad idea. The billiard ball is not.");
        } else {
            say("Charlie is tied to a chair. The thug would win that one.");
        }
        handled();
        return true;
    }
    if verb() == "search" && (noun() == "" || noun() == "guard") {
        if !flag("thugDown") {
            say("The thug is watching. Charlie does not put a hand in that coat.");
        } else {
            say(pocket_line());
        }
        handled();
        return true;
    }
    if verb() == "get" && loot_noun() {
        if !flag("thugDown") {
            say("The thug is still on his feet. Charlie keeps his hands behind his back.");
        } else if has(noun()) {
            say("Charlie already has that.");
        } else {
            take_item(noun());
            if noun() == "knife" {
                score("thug-knife", 2);
            } else if noun() == "chips" {
                score("thug-chips", 2);
            } else {
                score("thug-hanky", 2);
            }
            sfx("score");
            say(loot_look(noun()));
        }
        handled();
        return true;
    }
    false
}

fn dynamic_look() {
    false
}

fn carrying_line() {
    let parts = "";
    let ids = ["twenty", "ten", "five", "dollar", "quarters", "beer", "cologne", "iron", "umbrella", "matchbook", "pick", "note", "soap", "ball", "knife", "chips", "handkerchief", "chlorine", "bomb", "books"];
    let one = ["a twenty-dollar bill", "a ten-dollar bill", "a five-dollar bill", "a one-dollar bill", "a roll of quarters", "a cold beer", "a bottle of Midnight Stallion", "a tyre iron", "a hooked black umbrella", "a Purple Rhino matchbook", "a black hair pick", "a folded licence-plate note", "a slimy bar of soap", "a billiard ball", "a short knife", "a few casino chips", "a handkerchief", "a tin of chlorine tablets", "a smoke bomb", "a set of account books"];
    let many = ["twenty-dollar bills", "ten-dollar bills", "five-dollar bills", "one-dollar bills", "rolls of quarters", "cold beers", "bottles of Midnight Stallion", "tyre irons", "hooked black umbrellas", "Purple Rhino matchbooks", "black hair picks", "folded notes", "slimy bars of soap", "billiard balls", "short knives", "piles of casino chips", "handkerchiefs", "tins of chlorine tablets", "smoke bombs", "sets of account books"];
    for i in 0..20 {
        let n = count(ids[i]);
        if n == 1 {
            if parts == "" { parts = one[i]; } else { parts = parts + ", " + one[i]; }
        } else if n > 1 {
            let bit = n.to_string() + " " + many[i];
            if parts == "" { parts = bit; } else { parts = parts + ", " + bit; }
        }
    }
    if parts == "" {
        return "You are carrying nothing, which feels about right.";
    }
    "You are carrying: " + parts + "."
}

fn self_look() {
    if flag("wet") {
        return "Charlie looks down. He is dripping. The grey suit is a sponge, and he leaves a trail of water wherever he stands.";
    }
    if flag("dry") {
        return "Charlie is wearing the thug's dry clothes. They fit like a borrowed alibi, and they do not drip.";
    }
    "Charlie. Grey suit, blond hair, and the look of a man who wanted one drink."
}

fn on_command() {
    if police() {
        return;
    }
    let where = room();
    if where == "billiard" && billiard_travel() {
        return;
    } else if where == "deck" && deck_cmd() {
        return;
    } else if where == "hall" && hall_cmd() {
        return;
    } else if where == "kitchen" && kitchen_cmd() {
        return;
    } else if where == "hallway" && hallway_cmd() {
        return;
    } else if where == "office" && office_cmd() {
        return;
    }
    if dynamic_look() {
        return;
    }
    if books_anywhere() {
        return;
    }
    if where == "billiard" && billiard_cmd() {
        return;
    }
    if verb() == "inventory" {
        say(carrying_line());
        handled();
        return;
    }
    if verb() == "look" && noun() == "self" {
        say(self_look());
        handled();
        return;
    }
}

fn lake_lines() {
    say("Charlie comes to with his shoes dragging and a hand on each arm. Dana is beside him. The pencil is still in her bun. The red cardigan is crooked.");
    say("He checks his watch. He has been out about an hour.");
    say("He looks for the city skyline. There isn't one. Only trees, black water, and a dock.");
    say("The lights ahead belong to a big timber house. Vic's lakehouse.");
}

fn arrival_lines() {
    say("Charlie is sat in a chair. His hands are tied behind his back. He can't get his feet under him.");
    say("Dark wood, a pool table, a bar cart, antlers on the wall, and tall windows onto the night lake. One door.");
    say("A thug leans nearby, feet planted, a cigarette moving between his fingers.");
    say("Dana is not here. They took her to another room.");
    say("The goons never checked his pockets.");
    if !has("matchbook") {
        set_flag("floorBook");
        say("A matchbook lies on the floor by the chair. Charlie could reach it if he scooted, and if nobody was watching.");
    }
}

fn on_after() {
    if room() == "street" && verb() == "talk" && (noun() == "doorman" || noun() == "cab") {
        // The live runtime drops these pose flags at the start of a play command.
        clear_flag("professorTalk");
        clear_flag("tankStop");
        set_flag("quietEnter");
        set_flag("tied");
        hold(true);
        picture("door");
        lake_lines();
        arrival_lines();
        goto_room("billiard", "default");
        return;
    }
    if room() != "billiard" {
        return;
    }
    if flag("vicEntered") || flag("vicLeft") {
        return;
    }
    let n = num("billiardMoves") + 1;
    set_var("billiardMoves", n);
    let room_look = verb() == "look" && (noun() == "" || noun() == "around" || noun() == "room");
    if !room_look && n < 3 {
        return;
    }
    set_flag("vicEntered");
    set_flag("vicLeft");
    set_var("thug", 0);
    say("The door opens. A man in a pale suit and a dark shirt steps in. Vic.");
    say("Vic looks Charlie over and asks the thug, \"Is this the nosey Parker?\"");
    say("Vic leans in. He murmurs something to the thug. Charlie catches nothing but the tone, and the tone is bad.");
    say("Vic leaves through the door. The thug stays, smoking.");
}
