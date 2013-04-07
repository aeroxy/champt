$(document).ready(function() {
	function drawMe() {
		navigator.geolocation.watchPosition(function(position) {
			var latitude = position.coords.latitude;
			var longitude = position.coords.longitude;
			var gLatLng = new google.maps.LatLng(latitude, longitude);
			var nameLatLng = new google.maps.LatLng(latitude - 0.00004, longitude);
			var mapOptions = {
				center: gLatLng,
				zoom: 18,
				mapTypeId: google.maps.MapTypeId.ROADMAP,
				draggable: false,
				zoomControl: false,
				scrollwheel: false,
				disableDoubleClickZoom: true,
				disableDefaultUI: true,
			};
			var map = new google.maps.Map(document.getElementById("map-canvas"),mapOptions);
			var yourLocation = {
				path: google.maps.SymbolPath.CIRCLE,
				strokeColor: "#4c8f2a",
				strokeWeight: 2,
				fillColor: "#a2e346",
				fillOpacity: 1,
				scale: 6,
			};
			var drawCircle = new google.maps.Marker({
				position: gLatLng,
				icon: yourLocation,
				map: map,
			});
			var mapUserName = new MapLabel({
				text: 'Anonymous User',
				position: nameLatLng,
				map: map,
				fontSize: 14,
				fontColor: "#ffffff",
				strokeWeight: 4,
				strokeColor: "4c8f2a",
				fontFamily: "Avenir",
			});
			var changeUserName = document.getElementById('change-username');
			google.maps.event.addDomListener(changeUserName, 'click', function() {
				mapUserName.set('text', document.getElementById('userName').value);
			});

//fuck
			// var contentString = '<div id="content">'+
			// '<div id="siteNotice">'+
			// '</div>'+
			// '<h2 id="firstHeading" class="firstHeading">Uluru</h2>'+
			// '<div id="bodyContent">'+
			// '<p><b>Uluru</b>, also referred to as <b>Ayers Rock</b>, is a large ' +
			// 'sandstone rock formation in the southern part of the '+
			// 'Northern Territory, central Australia. It lies 335 km (208 mi) '+
			// 'south west of the nearest large town, Alice Springs; 450 km '+
			// '(280 mi) by road. Kata Tjuta and Uluru are the two major '+
			// 'features of the Uluru - Kata Tjuta National Park. Uluru is '+
			// 'sacred to the Pitjantjatjara and Yankunytjatjara, the '+
			// 'Aboriginal people of the area. It has many springs, waterholes, '+
			// 'rock caves and ancient paintings. Uluru is listed as a World '+
			// 'Heritage Site.</p>'+
			// '<p>Attribution: Uluru, <a href="http://en.wikipedia.org/w/index.php?title=Uluru&oldid=297882194">'+
			// 'http://en.wikipedia.org/w/index.php?title=Uluru</a> (last visited June 22, 2009).</p>'+
			// '</div>'+
			// '</div>';

			var infoBubble = new InfoBubble({
				content: '<div class="bubble"><p>I feel awesome today!</p></div>',
				position: gLatLng,
				maxWidth: 300,
				hideCloseButton: true,
				shadowStyle: 0,
				backgroundColor: 'rgba(57,57,57,0.8)',
				backgroundClassName: 'bubble',
			});

			infoBubble.open(map);

			// google.maps.event.addListener(yourLocation, 'click', function() {
			// 	if (!infoBubble.isOpen()) {
			// 		infoBubble.open(map, yourLocation);
			// 	}
			// });

//fuck
		}, function(error) {
			alert('Error occurred. Error code: ' + error.code);
		});
	};
	drawMe();

	function resizeText() {
		var windowHeight = $(window).height();
		var windowWidth = $(window).width();
		$("footer").css({
			'padding-top': windowHeight * 0.15 * 0.1 + 'px',
		});
	};
	resizeText();

	$(window).resize(function(){
		drawMe();
		resizeText();
	});
});